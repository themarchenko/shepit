//! Owns the widget window: sizing, placement, dragging, hover and the click and hold gestures.
//!
//! Every change goes through one `Mutex<Inner>`. The placement closure is posted to the main thread
//! while that lock is still held, so closures run in the order the changes were made. The closure
//! places the window first and emits the state second, so the frontend never gets a shape that the
//! window cannot hold yet.
//!
//! Hover and pointer events arrive through sync commands, which Tauri runs in IPC order, and are only
//! queued there. One thread drains the queue, so a fast click's down is always handled before its up.
//!
//! The window must never be queried from the main thread while a worker holds the lock and waits
//! for the main thread (monitor and cursor queries block on it). Menu handlers therefore hand their
//! work to a thread instead of calling in directly.

use crate::controller::Msg;
use crate::settings_store::SettingsStore;
use crate::widget::{self, clamped_anchor, Presentation, WidgetState};
use shepit_core::dictation::{Phase, View, WidgetInput};
use shepit_core::settings::{changes, Changes, Settings};
use shepit_core::widget::{
    default_anchor, resolve_anchor, Fitter, Monitor, Point, PointerGesture, PointerOut, Rect,
    Shrink, Size, HOLD_SECS, SHRINK_DELAY_SECS,
};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

const LABEL: &str = "widget";

#[derive(Clone)]
pub struct WidgetHost {
    inner: Arc<Mutex<Inner>>,
    msgs: SyncSender<Msg>,
    /// Hover and pointer events from the frontend, in arrival order.
    ui: Sender<UiEvent>,
    /// `livePreview`, shared with the controller thread. `settings_store::apply` stores it.
    live: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
}

impl PointerKind {
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "down" => Some(Self::Down),
            "move" => Some(Self::Move),
            "up" => Some(Self::Up),
            _ => None,
        }
    }
}

enum UiEvent {
    Hover(bool),
    Pointer(PointerKind),
}

/// Feeds one pointer event to the gesture. The flag is true when a press began and the hold timer
/// must start.
fn step_gesture(
    gesture: &mut PointerGesture,
    kind: PointerKind,
    at: f64,
    x: f64,
    y: f64,
    scale: f64,
) -> (PointerOut, bool) {
    match kind {
        PointerKind::Down => {
            *gesture = PointerGesture::new(scale);
            (gesture.down(at, x, y), true)
        }
        PointerKind::Move => (gesture.moved(x, y), false),
        PointerKind::Up => (gesture.up(at), false),
    }
}

struct Inner {
    app: AppHandle,
    epoch: Instant,
    view: View,
    presentation: Presentation,
    settings: Settings,
    store: SettingsStore,
    fitter: Fitter,
    /// The last size asked of the fitter. Level updates keep the size, and asking again would bump
    /// the fitter generation and cancel a shrink that is still waiting.
    target: Size,
    /// Bottom centre of the window, physical pixels.
    anchor: Point,
    gesture: PointerGesture,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
}

impl WidgetHost {
    pub fn new(app: AppHandle, msgs: SyncSender<Msg>, mut store: SettingsStore) -> Self {
        let settings = store.load();
        let live = Arc::new(AtomicBool::new(settings.live_preview));
        let presentation = presentation_of(&settings);
        let view = widget::idle_view();
        let target = widget::window_for(&view, &presentation);
        let anchor = resolve_anchor(settings.widget_anchor, &monitors(&app));
        let inner = Inner {
            app,
            epoch: Instant::now(),
            view,
            presentation,
            settings,
            store,
            fitter: Fitter::new(target),
            target,
            anchor,
            gesture: PointerGesture::new(1.0),
        };
        let (ui, queue) = mpsc::channel();
        let host = Self {
            inner: Arc::new(Mutex::new(inner)),
            msgs,
            ui,
            live,
        };
        let worker = host.clone();
        std::thread::spawn(move || worker.drain(queue));
        host
    }

    /// Queues a hover change. Never blocks, so it is safe on the main thread.
    pub fn push_hover(&self, hovering: bool) {
        let _ = self.ui.send(UiEvent::Hover(hovering));
    }

    /// Queues a pointer event ("down", "move" or "up"). Never blocks.
    pub fn push_pointer(&self, kind: &str) -> Result<(), String> {
        let kind =
            PointerKind::parse(kind).ok_or_else(|| format!("unknown pointer event: {kind}"))?;
        let _ = self.ui.send(UiEvent::Pointer(kind));
        Ok(())
    }

    /// The one thread that handles frontend events. It may block on the main thread (cursor and
    /// monitor queries) and on the controller channel; the main thread never waits for it.
    fn drain(&self, queue: Receiver<UiEvent>) {
        for event in queue {
            match event {
                UiEvent::Hover(hovering) => self.hover(hovering),
                UiEvent::Pointer(kind) => self.pointer(kind),
            }
        }
    }

    pub fn visible(&self) -> bool {
        lock(&self.inner).settings.widget_visible
    }

    /// Places the window, sends the first state and shows the window when the setting says so.
    pub fn start(&self) {
        let mut inner = lock(&self.inner);
        inner.post_full();
        if inner.settings.widget_visible {
            inner.post_visibility(true);
        }
    }

    /// The frontend finished loading and listens for `widget-state`.
    pub fn ready(&self) {
        lock(&self.inner).post_full();
    }

    pub fn view(&self, view: View) {
        let later = {
            let mut inner = lock(&self.inner);
            inner.view = view;
            inner.present()
        };
        self.spawn_shrink(later);
    }

    pub fn hover(&self, hovering: bool) {
        let later = {
            let mut inner = lock(&self.inner);
            if inner.presentation.hovering == hovering {
                return;
            }
            inner.presentation.hovering = hovering;
            inner.present()
        };
        self.spawn_shrink(later);
    }

    /// The live preview switch the controller reads at the start of a recording.
    pub fn live_preview(&self) -> Arc<AtomicBool> {
        self.live.clone()
    }

    /// A copy of the settings as they are now.
    pub fn settings(&self) -> Settings {
        lock(&self.inner).settings.clone()
    }

    /// Replaces the settings with what `edit` makes of the current ones, under the lock, so a
    /// drag that saves the anchor at the same moment is not overwritten. Saves, then refreshes the
    /// widget: the look and key label when they changed, show or hide when `widgetVisible` did.
    /// `edit` must not call back into the host.
    pub fn update(
        &self,
        edit: impl FnOnce(&Settings) -> Result<Settings, String>,
    ) -> Result<(Settings, Changes), String> {
        let (new, changed, later) = {
            let mut inner = lock(&self.inner);
            let new = edit(&inner.settings)?;
            let changed = changes(&inner.settings, &new);
            let differs = new != inner.settings;
            inner.settings = new.clone();
            if differs {
                inner.save();
            }
            let mut later = None;
            if changed.look || changed.hotkey {
                inner.presentation = Presentation {
                    hovering: inner.presentation.hovering,
                    ..presentation_of(&new)
                };
                later = inner.present();
            }
            if changed.visible {
                inner.post_visibility(new.widget_visible);
            }
            (new, changed, later)
        };
        self.spawn_shrink(later);
        Ok((new, changed))
    }

    /// Queues `msg` for the controller. Never blocks: a full channel is an error.
    pub fn send(&self, msg: Msg) -> Result<(), String> {
        self.msgs
            .try_send(msg)
            .map_err(|_| "Shepit is busy, try again".to_string())
    }

    /// Hands `msg` to the controller whatever the state of its channel: a blocking send on a short-lived
    /// thread, so neither the caller (the main thread may be waiting on the host lock) nor the lock is
    /// ever held up. For messages that must not be lost, such as a changed key.
    pub fn send_reliably(&self, msg: Msg) {
        send_on_thread(self.msgs.clone(), msg);
    }

    /// True while a dictation is in progress (anything but idle).
    pub fn dictating(&self) -> bool {
        lock(&self.inner).view.phase != Phase::Idle
    }

    /// Back to the default spot on the primary monitor; forgets the saved anchor.
    pub fn reset_position(&self) {
        let mut inner = lock(&self.inner);
        if let Some(primary) = monitors(&inner.app).first() {
            inner.anchor = default_anchor(primary);
        }
        inner.settings.widget_anchor = None;
        inner.save();
        inner.post_full();
    }

    /// The cursor is read here, in physical pixels, because the window moves under a drag and event
    /// coordinates would shift with it.
    fn pointer(&self, kind: PointerKind) {
        let app = lock(&self.inner).app.clone();
        let cursor = match app.cursor_position() {
            Ok(c) => c,
            Err(err) => {
                eprintln!("could not read the cursor: {err}");
                return;
            }
        };
        let (out, pressed) = {
            let mut inner = lock(&self.inner);
            let at = inner.epoch.elapsed().as_secs_f64();
            let scale = if kind == PointerKind::Down {
                inner.scale()
            } else {
                1.0
            };
            step_gesture(&mut inner.gesture, kind, at, cursor.x, cursor.y, scale)
        };
        if pressed {
            self.spawn_hold_timer();
        }
        self.finish(out);
    }

    fn finish(&self, out: PointerOut) {
        let msg = lock(&self.inner).handle(out);
        // Outside the lock: a full channel must not stall the controller's own calls into us.
        if let Some(input) = msg {
            let _ = self.msgs.send(Msg::Widget(input));
        }
    }

    /// The core gesture has no clock of its own; this thread is the 0.35 s timer.
    fn spawn_hold_timer(&self) {
        let host = self.clone();
        std::thread::spawn(move || {
            // A little over the threshold so the core's `now - at >= HOLD_SECS` is true.
            std::thread::sleep(Duration::from_secs_f64(HOLD_SECS + 0.01));
            let out = {
                let mut inner = lock(&host.inner);
                let now = inner.epoch.elapsed().as_secs_f64();
                inner.gesture.tick(now)
            };
            host.finish(out);
        });
    }

    fn spawn_shrink(&self, later: Option<Shrink>) {
        let Some(shrink) = later else { return };
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(SHRINK_DELAY_SECS));
            let mut inner = lock(&inner);
            // None when a newer request came in: that one owns the next shrink.
            if let Some(size) = inner.fitter.shrink_due(shrink.generation) {
                inner.post_frame(size, None);
            }
        });
    }
}

fn presentation_of(settings: &Settings) -> Presentation {
    Presentation {
        style: settings.widget_style,
        glass_opacity: settings.glass_opacity,
        fade_when_idle: settings.fade_when_idle,
        hovering: false,
        hotkey: settings.hotkey,
    }
}

impl Inner {
    fn monitors(&self) -> Vec<Monitor> {
        monitors(&self.app)
    }

    fn scale(&self) -> f64 {
        let all = self.monitors();
        shepit_core::widget::monitor_for(self.anchor, &all)
            .and_then(|i| all.get(i))
            .map_or(1.0, |m| m.scale)
    }

    fn state(&self) -> WidgetState {
        WidgetState::new(&self.view, &self.presentation)
    }

    /// Sends the current state; resizes first when the layout size changed. Returns the shrink to
    /// schedule when the window had to stay larger than the new layout for now.
    fn present(&mut self) -> Option<Shrink> {
        let target = widget::window_for(&self.view, &self.presentation);
        let state = self.state();
        if target == self.target {
            self.post(None, Some(state));
            return None;
        }
        self.target = target;
        let fit = self.fitter.request(target);
        self.post_frame(fit.now, Some(state));
        fit.later
    }

    /// Places the window at the current size and sends the state, whatever changed.
    fn post_full(&mut self) {
        let size = self.fitter.current();
        let state = self.state();
        self.post_frame(size, Some(state));
    }

    fn post_frame(&self, size: Size, state: Option<WidgetState>) {
        let frame = widget::placement(self.anchor, size, &self.monitors());
        self.post(frame, state);
    }

    fn post(&self, frame: Option<Rect>, state: Option<WidgetState>) {
        let app = self.app.clone();
        let posted = self.app.run_on_main_thread(move || {
            if let (Some(frame), Some(window)) = (frame, app.get_webview_window(LABEL)) {
                if let Err(err) = set_frame(&window, frame) {
                    eprintln!("could not place the widget: {err}");
                }
            }
            if let Some(state) = state {
                if let Err(err) = app.emit_to(LABEL, "widget-state", state) {
                    eprintln!("could not send the widget state: {err}");
                }
            }
        });
        if let Err(err) = posted {
            eprintln!("could not reach the main thread: {err}");
        }
    }

    fn post_visibility(&self, visible: bool) {
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            let Some(window) = app.get_webview_window(LABEL) else {
                return;
            };
            let result = if visible {
                window.show()
            } else {
                window.hide()
            };
            if let Err(err) = result {
                eprintln!("could not change the widget visibility: {err}");
            }
        });
    }

    fn save(&mut self) {
        if let Err(err) = self.store.save(&self.settings) {
            eprintln!("could not save the settings: {err}");
        }
    }

    /// Applies a gesture result. A click or hold is returned for the controller.
    fn handle(&mut self, out: PointerOut) -> Option<WidgetInput> {
        match out {
            PointerOut::None => None,
            PointerOut::Click => Some(WidgetInput::Click),
            PointerOut::HoldBegan => Some(WidgetInput::HoldBegan),
            PointerOut::HoldEnded => Some(WidgetInput::HoldEnded),
            PointerOut::Dragged { dx, dy } => {
                self.anchor.x += dx.round() as i32;
                self.anchor.y += dy.round() as i32;
                let size = self.fitter.current();
                self.post_frame(size, None);
                None
            }
            PointerOut::DragEnded => {
                // Dropped off every monitor: back to the default spot rather than out of reach.
                let monitors = self.monitors();
                let size = self.fitter.current();
                // Store where the window really sits, clamped into the work area, as macOS stores the
                // clamped frame (WidgetController.swift:187-190).
                self.anchor = clamped_anchor(
                    resolve_anchor(Some(self.anchor), &monitors),
                    size,
                    &monitors,
                );
                self.settings.widget_anchor = Some(self.anchor);
                self.save();
                self.post_frame(size, None);
                None
            }
        }
    }
}

/// Work areas in physical pixels, primary first (the core treats index 0 as the primary).
fn monitors(app: &AppHandle) -> Vec<Monitor> {
    let convert = |m: &tauri::window::Monitor| {
        let area = m.work_area();
        Monitor {
            work: Rect {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width as i32,
                height: area.size.height as i32,
            },
            scale: m.scale_factor(),
        }
    };
    let mut out = Vec::new();
    if let Ok(Some(primary)) = app.primary_monitor() {
        out.push(convert(&primary));
    }
    for m in app.available_monitors().unwrap_or_default() {
        let c = convert(&m);
        if !out.iter().any(|o| o.work == c.work) {
            out.push(c);
        }
    }
    out
}

/// The blocking send of `send_reliably`; a closed channel (the app is quitting) just ends the thread.
fn send_on_thread(tx: SyncSender<Msg>, msg: Msg) {
    std::thread::spawn(move || {
        let _ = tx.send(msg);
    });
}

/// One call moves and resizes the window, so the shape never shows a frame at the old size.
#[cfg(windows)]
pub(crate) fn set_frame(window: &WebviewWindow, frame: Rect) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    // SAFETY: the handle belongs to the live widget window; the call only reads its arguments.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            frame.x,
            frame.y,
            frame.width,
            frame.height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
    .map_err(|e| e.to_string())
}

/// Two calls, so a frame may show the old size. Only for builds that cannot run on Windows.
#[cfg(not(windows))]
pub(crate) fn set_frame(window: &WebviewWindow, frame: Rect) -> Result<(), String> {
    use tauri::{PhysicalPosition, PhysicalSize};
    window
        .set_size(PhysicalSize::new(frame.width as u32, frame.height as u32))
        .map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(frame.x, frame.y))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::keys::Hotkey;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn a_message_for_a_full_channel_is_still_delivered() {
        let (tx, rx) = mpsc::sync_channel::<Msg>(1);
        tx.try_send(Msg::Tick).unwrap();
        // The channel is full: a try_send would fail here.
        send_on_thread(tx, Msg::Hotkey(Hotkey::RightAlt));
        assert!(matches!(rx.recv().unwrap(), Msg::Tick));
        let got = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(got, Msg::Hotkey(Hotkey::RightAlt)));
    }

    fn run(events: &[(PointerKind, f64, f64, f64)]) -> Vec<PointerOut> {
        let mut gesture = PointerGesture::new(1.0);
        events
            .iter()
            .map(|&(kind, at, x, y)| step_gesture(&mut gesture, kind, at, x, y, 1.0).0)
            .collect()
    }

    #[test]
    fn kinds_parse_and_unknown_ones_are_refused() {
        assert_eq!(PointerKind::parse("down"), Some(PointerKind::Down));
        assert_eq!(PointerKind::parse("move"), Some(PointerKind::Move));
        assert_eq!(PointerKind::parse("up"), Some(PointerKind::Up));
        assert_eq!(PointerKind::parse("click"), None);
    }

    #[test]
    fn a_fast_down_then_up_is_one_click_and_leaves_no_press() {
        let mut gesture = PointerGesture::new(1.0);
        let (out, pressed) = step_gesture(&mut gesture, PointerKind::Down, 1.0, 5.0, 5.0, 1.0);
        assert_eq!((out, pressed), (PointerOut::None, true));
        let (out, pressed) = step_gesture(&mut gesture, PointerKind::Up, 1.05, 5.0, 5.0, 1.0);
        assert_eq!((out, pressed), (PointerOut::Click, false));
        // The 0.35 s timer of that press fires later and finds nothing to begin.
        assert_eq!(gesture.tick(1.4), PointerOut::None);
    }

    #[test]
    fn a_held_press_begins_and_ends_once() {
        let mut gesture = PointerGesture::new(1.0);
        step_gesture(&mut gesture, PointerKind::Down, 1.0, 5.0, 5.0, 1.0);
        assert_eq!(gesture.tick(1.4), PointerOut::HoldBegan);
        let (out, _) = step_gesture(&mut gesture, PointerKind::Up, 2.0, 5.0, 5.0, 1.0);
        assert_eq!(out, PointerOut::HoldEnded);
    }

    #[test]
    fn a_drag_reports_the_move_and_its_end() {
        let out = run(&[
            (PointerKind::Down, 1.0, 0.0, 0.0),
            (PointerKind::Move, 1.1, 20.0, 0.0),
            (PointerKind::Up, 1.2, 20.0, 0.0),
        ]);
        assert_eq!(out[1], PointerOut::Dragged { dx: 20.0, dy: 0.0 });
        assert_eq!(out[2], PointerOut::DragEnded);
    }

    #[test]
    fn the_presentation_carries_the_look_and_the_hotkey() {
        let settings = Settings {
            widget_style: shepit_core::widget::Style::Island,
            glass_opacity: 0.55,
            fade_when_idle: false,
            hotkey: shepit_core::keys::Hotkey::F19,
            ..Settings::default()
        };
        let p = presentation_of(&settings);
        assert_eq!(p.style, shepit_core::widget::Style::Island);
        assert_eq!(p.glass_opacity, 0.55);
        assert!(!p.fade_when_idle);
        assert!(!p.hovering);
        assert_eq!(p.hotkey, shepit_core::keys::Hotkey::F19);
    }
}
