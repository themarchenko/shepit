//! Owns the widget window: sizing, placement, dragging, hover and the click and hold gestures.
//!
//! Every change goes through one `Mutex<Inner>`. The placement closure is posted to the main thread
//! while that lock is still held, so closures run in the order the changes were made. The closure
//! places the window first and emits the state second, so the frontend never gets a shape that the
//! window cannot hold yet.
//!
//! The window must never be queried from the main thread while a worker holds the lock and waits
//! for the main thread (monitor and cursor queries block on it). Menu handlers therefore hand their
//! work to a thread instead of calling in directly.

use crate::controller::Msg;
use crate::settings_store::SettingsStore;
use crate::widget::{self, Presentation, WidgetState};
use shepit_core::dictation::{View, WidgetInput};
use shepit_core::settings::Settings;
use shepit_core::widget::{
    default_anchor, resolve_anchor, Fitter, Monitor, Point, PointerGesture, PointerOut, Rect,
    Shrink, Size, HOLD_SECS, SHRINK_DELAY_SECS,
};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

const LABEL: &str = "widget";

#[derive(Clone)]
pub struct WidgetHost {
    inner: Arc<Mutex<Inner>>,
    msgs: SyncSender<Msg>,
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
        Self {
            inner: Arc::new(Mutex::new(inner)),
            msgs,
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

    /// Flips "Show widget", saves it and returns the new value.
    pub fn toggle_visible(&self) -> bool {
        let mut inner = lock(&self.inner);
        let visible = !inner.settings.widget_visible;
        inner.settings.widget_visible = visible;
        inner.save();
        inner.post_visibility(visible);
        visible
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

    /// `kind` is "down", "move" or "up". The cursor is read here, in physical pixels, because the
    /// window moves under a drag and event coordinates would shift with it.
    pub fn pointer(&self, kind: &str) -> Result<(), String> {
        if !matches!(kind, "down" | "move" | "up") {
            return Err(format!("unknown pointer event: {kind}"));
        }
        let app = lock(&self.inner).app.clone();
        let cursor = app.cursor_position().map_err(|e| e.to_string())?;
        let (x, y) = (cursor.x, cursor.y);
        let (out, pressed) = {
            let mut inner = lock(&self.inner);
            let at = inner.epoch.elapsed().as_secs_f64();
            match kind {
                "down" => {
                    let scale = inner.scale();
                    inner.gesture = PointerGesture::new(scale);
                    (inner.gesture.down(at, x, y), true)
                }
                "move" => (inner.gesture.moved(x, y), false),
                _ => (inner.gesture.up(at), false),
            }
        };
        if pressed {
            self.spawn_hold_timer();
        }
        self.finish(out);
        Ok(())
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
                self.anchor = resolve_anchor(Some(self.anchor), &self.monitors());
                self.settings.widget_anchor = Some(self.anchor);
                self.save();
                let size = self.fitter.current();
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

/// One call moves and resizes the window, so the shape never shows a frame at the old size.
#[cfg(windows)]
fn set_frame(window: &WebviewWindow, frame: Rect) -> Result<(), String> {
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
fn set_frame(window: &WebviewWindow, frame: Rect) -> Result<(), String> {
    use tauri::{PhysicalPosition, PhysicalSize};
    window
        .set_size(PhysicalSize::new(frame.width as u32, frame.height as u32))
        .map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(frame.x, frame.y))
        .map_err(|e| e.to_string())
}
