//! Dictation session for the Windows port: key events -> gesture -> recorder, per
//! docs/spec/hotkey.md:43-56. No recognition or paste yet.

use crate::audio::{is_long_enough, smooth_level, LevelHistory};
use crate::gesture::{HotkeyGesture, Output};
use crate::keys::{KeyEvent, KeyFilter, RawKey};

/// Failed state resets to idle after 4 s (DictationSession.swift:234).
pub const FAILED_RESET_SECS: f64 = 4.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    NoDevice,
    AccessDenied,
    Other(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::NoDevice => write!(f, "no microphone found"),
            CaptureError::AccessDenied => write!(
                f,
                "access denied, check Windows microphone privacy settings"
            ),
            CaptureError::Other(text) => write!(f, "{text}"),
        }
    }
}

/// Maps a cpal error text to a variant: 0x80070005, E_ACCESSDENIED or "access is denied" (any
/// case) -> AccessDenied; a "no default device" text -> NoDevice; else Other.
pub fn classify_error(text: &str) -> CaptureError {
    let lower = text.to_lowercase();
    if lower.contains("0x80070005")
        || lower.contains("e_accessdenied")
        || lower.contains("access is denied")
    {
        CaptureError::AccessDenied
    } else if lower.contains("no default") || lower.contains("no device") {
        CaptureError::NoDevice
    } else {
        CaptureError::Other(text.to_string())
    }
}

pub trait Recorder {
    fn start(&mut self) -> Result<(), CaptureError>;
    /// Stops capture and returns 16 kHz mono samples.
    fn stop(&mut self) -> Vec<f32>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Idle,
    Recording { hands_free: bool },
    Failed { message: String },
}

/// Gesture and key outputs that reach the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    PressBegan,
    PressEnded,
    ShortTap,
    DoubleTap,
    Esc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Start,
    Stop,
    Cancel,
    MakeHandsFree,
}

/// The table of hotkey.md:43-49.
pub fn action_for(state: &State, event: Event) -> Action {
    match (state, event) {
        (State::Recording { hands_free: true }, Event::PressBegan) => Action::Stop,
        (State::Recording { hands_free: false }, Event::PressBegan) => Action::None,
        (_, Event::PressBegan) => Action::Start,
        (State::Recording { .. }, Event::PressEnded) => Action::Stop,
        (State::Recording { hands_free: false }, Event::ShortTap) => Action::Cancel,
        (State::Recording { hands_free: false }, Event::DoubleTap) => Action::MakeHandsFree,
        (State::Recording { .. }, Event::Esc) => Action::Cancel,
        _ => Action::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Listening,
    Failed,
}

/// What the widget shows. `levels` always has 32 entries, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub phase: Phase,
    pub hands_free: bool,
    pub message: String,
    pub levels: Vec<f32>,
}

pub struct Controller<R: Recorder> {
    recorder: R,
    filter: KeyFilter,
    gesture: HotkeyGesture,
    state: State,
    failed_at: f64,
    capture: Option<Vec<f32>>,
    smoothed: f32,
    history: LevelHistory,
}

impl<R: Recorder> Controller<R> {
    pub fn new(recorder: R) -> Self {
        Self {
            recorder,
            filter: KeyFilter::new(),
            gesture: HotkeyGesture::new(),
            state: State::Idle,
            failed_at: 0.0,
            capture: None,
            smoothed: 0.0,
            history: LevelHistory::new(),
        }
    }

    /// `at` is seconds on a monotonic clock, taken in the hook callback.
    pub fn on_key(&mut self, raw: RawKey, at: f64) {
        let event = match self.filter.filter(raw) {
            Some(KeyEvent::HotkeyDown) => match self.gesture.key_down(at) {
                Output::PressBegan => Event::PressBegan,
                _ => return,
            },
            Some(KeyEvent::HotkeyUp) => match self.gesture.key_up(at) {
                Output::PressEnded { .. } => Event::PressEnded,
                Output::ShortTap => Event::ShortTap,
                Output::DoubleTap => Event::DoubleTap,
                _ => return,
            },
            Some(KeyEvent::EscPressed) => Event::Esc,
            None => return,
        };
        match action_for(&self.state, event) {
            Action::None => {}
            Action::Start => self.start(at),
            Action::Stop => {
                let samples = self.stop_recorder();
                // Too short to be speech: dropped silently (DictationSession.swift:49).
                if is_long_enough(samples.len()) {
                    self.capture = Some(samples);
                }
            }
            Action::Cancel => {
                self.stop_recorder();
            }
            Action::MakeHandsFree => self.state = State::Recording { hands_free: true },
        }
    }

    fn start(&mut self, at: f64) {
        self.capture = None;
        self.clear_levels();
        match self.recorder.start() {
            Ok(()) => self.state = State::Recording { hands_free: false },
            Err(e) => {
                self.state = State::Failed {
                    message: format!("Microphone: {e}"),
                };
                self.failed_at = at;
            }
        }
    }

    fn stop_recorder(&mut self) -> Vec<f32> {
        let samples = self.recorder.stop();
        self.state = State::Idle;
        self.clear_levels();
        samples
    }

    fn clear_levels(&mut self) {
        self.smoothed = 0.0;
        self.history.clear();
    }

    /// A raw level from the capture callback. Ignored unless recording; smoothed here.
    pub fn on_level(&mut self, raw: f32) {
        if matches!(self.state, State::Recording { .. }) {
            self.smoothed = smooth_level(self.smoothed, raw);
            self.history.push(self.smoothed);
        }
    }

    /// Resets Failed to idle once `FAILED_RESET_SECS` have passed.
    pub fn poll(&mut self, now: f64) {
        if matches!(self.state, State::Failed { .. }) && now - self.failed_at >= FAILED_RESET_SECS {
            self.state = State::Idle;
        }
    }

    pub fn view(&self) -> View {
        let (phase, hands_free, message) = match &self.state {
            State::Idle => (Phase::Idle, false, String::new()),
            State::Recording { hands_free } => (Phase::Listening, *hands_free, String::new()),
            State::Failed { message } => (Phase::Failed, false, message.clone()),
        };
        View {
            phase,
            hands_free,
            message,
            levels: self.history.levels().to_vec(),
        }
    }

    /// The last kept recording, handed over once.
    pub fn take_capture(&mut self) -> Option<Vec<f32>> {
        self.capture.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{LLKHF_EXTENDED, VK_ESCAPE, VK_RCONTROL, WM_KEYDOWN, WM_KEYUP};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Default)]
    struct Shared {
        starts: Cell<u32>,
        stops: Cell<u32>,
        start_error: RefCell<Option<CaptureError>>,
        stop_len: Cell<usize>,
    }

    struct FakeRecorder(Rc<Shared>);

    impl Recorder for FakeRecorder {
        fn start(&mut self) -> Result<(), CaptureError> {
            self.0.starts.set(self.0.starts.get() + 1);
            match self.0.start_error.borrow().clone() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }
        fn stop(&mut self) -> Vec<f32> {
            self.0.stops.set(self.0.stops.get() + 1);
            vec![0.1; self.0.stop_len.get()]
        }
    }

    fn setup() -> (Controller<FakeRecorder>, Rc<Shared>) {
        let shared = Rc::new(Shared::default());
        shared.stop_len.set(16_000);
        (Controller::new(FakeRecorder(shared.clone())), shared)
    }

    fn press(c: &mut Controller<FakeRecorder>, at: f64) {
        c.on_key(
            RawKey {
                message: WM_KEYDOWN,
                vk: VK_RCONTROL,
                flags: LLKHF_EXTENDED,
            },
            at,
        );
    }
    fn release(c: &mut Controller<FakeRecorder>, at: f64) {
        c.on_key(
            RawKey {
                message: WM_KEYUP,
                vk: VK_RCONTROL,
                flags: LLKHF_EXTENDED,
            },
            at,
        );
    }
    fn esc(c: &mut Controller<FakeRecorder>, at: f64) {
        c.on_key(
            RawKey {
                message: WM_KEYDOWN,
                vk: VK_ESCAPE,
                flags: 0,
            },
            at,
        );
        c.on_key(
            RawKey {
                message: WM_KEYUP,
                vk: VK_ESCAPE,
                flags: 0,
            },
            at,
        );
    }
    fn recording(hands_free: bool) -> State {
        State::Recording { hands_free }
    }

    // The table, hotkey.md:43-49.

    #[test]
    fn press_began_starts_from_idle_and_failed() {
        let failed = State::Failed {
            message: "x".into(),
        };
        assert_eq!(action_for(&State::Idle, Event::PressBegan), Action::Start);
        assert_eq!(action_for(&failed, Event::PressBegan), Action::Start);
    }

    #[test]
    fn hands_free_stops_on_next_press() {
        assert_eq!(
            action_for(&recording(true), Event::PressBegan),
            Action::Stop
        );
    }

    #[test]
    fn press_began_is_ignored_while_recording_push_to_talk() {
        assert_eq!(
            action_for(&recording(false), Event::PressBegan),
            Action::None
        );
    }

    #[test]
    fn press_ended_stops_a_recording() {
        assert_eq!(
            action_for(&recording(false), Event::PressEnded),
            Action::Stop
        );
    }

    #[test]
    fn short_tap_cancels_only_a_push_to_talk_recording() {
        assert_eq!(
            action_for(&recording(false), Event::ShortTap),
            Action::Cancel
        );
        assert_eq!(action_for(&recording(true), Event::ShortTap), Action::None);
        assert_eq!(action_for(&State::Idle, Event::ShortTap), Action::None);
    }

    #[test]
    fn double_tap_makes_a_recording_hands_free() {
        assert_eq!(
            action_for(&recording(false), Event::DoubleTap),
            Action::MakeHandsFree
        );
    }

    #[test]
    fn esc_cancels_a_recording_and_does_nothing_when_idle() {
        assert_eq!(action_for(&recording(false), Event::Esc), Action::Cancel);
        assert_eq!(action_for(&recording(true), Event::Esc), Action::Cancel);
        assert_eq!(action_for(&State::Idle, Event::Esc), Action::None);
    }

    // The controller.

    #[test]
    fn one_second_hold_keeps_16000_samples() {
        let (mut c, shared) = setup();
        press(&mut c, 0.0);
        assert_eq!(c.view().phase, Phase::Listening);
        release(&mut c, 1.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(shared.stops.get(), 1);
        assert_eq!(c.take_capture().map(|s| s.len()), Some(16_000));
        assert_eq!(c.take_capture(), None);
    }

    #[test]
    fn recording_under_0_4_seconds_is_dropped() {
        let (mut c, shared) = setup();
        shared.stop_len.set(6399);
        press(&mut c, 0.0);
        release(&mut c, 1.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_capture(), None);
    }

    #[test]
    fn recording_of_exactly_0_4_seconds_is_kept() {
        let (mut c, shared) = setup();
        shared.stop_len.set(6400);
        press(&mut c, 0.0);
        release(&mut c, 1.0);
        assert_eq!(c.take_capture().map(|s| s.len()), Some(6400));
    }

    #[test]
    fn short_tap_starts_then_stops_and_keeps_nothing() {
        let (mut c, shared) = setup();
        press(&mut c, 0.0);
        release(&mut c, 0.1);
        assert_eq!((shared.starts.get(), shared.stops.get()), (1, 1));
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_capture(), None);
    }

    #[test]
    fn double_tap_goes_hands_free_and_the_next_press_stops_with_audio() {
        let (mut c, shared) = setup();
        press(&mut c, 0.0);
        release(&mut c, 0.1);
        press(&mut c, 0.2);
        release(&mut c, 0.3);
        let v = c.view();
        assert_eq!((v.phase, v.hands_free), (Phase::Listening, true));
        assert_eq!(shared.starts.get(), 2);
        press(&mut c, 2.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_capture().map(|s| s.len()), Some(16_000));
    }

    #[test]
    fn esc_while_recording_cancels_without_capture() {
        let (mut c, shared) = setup();
        press(&mut c, 0.0);
        esc(&mut c, 0.5);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(shared.stops.get(), 1);
        release(&mut c, 1.0);
        assert_eq!(c.take_capture(), None);
    }

    #[test]
    fn esc_while_idle_leaves_the_recorder_untouched() {
        let (mut c, shared) = setup();
        esc(&mut c, 0.0);
        assert_eq!((shared.starts.get(), shared.stops.get()), (0, 0));
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn right_ctrl_c_hold_keeps_audio() {
        let (mut c, _shared) = setup();
        press(&mut c, 0.0);
        c.on_key(
            RawKey {
                message: WM_KEYDOWN,
                vk: 0x43,
                flags: 0,
            },
            0.5,
        );
        c.on_key(
            RawKey {
                message: WM_KEYUP,
                vk: 0x43,
                flags: 0,
            },
            0.6,
        );
        release(&mut c, 1.0);
        assert_eq!(c.take_capture().map(|s| s.len()), Some(16_000));
    }

    #[test]
    fn start_error_fails_with_a_microphone_message() {
        let (mut c, shared) = setup();
        *shared.start_error.borrow_mut() = Some(CaptureError::Other("boom".into()));
        press(&mut c, 0.0);
        let v = c.view();
        assert_eq!(v.phase, Phase::Failed);
        assert_eq!(v.message, "Microphone: boom");
    }

    #[test]
    fn failed_resets_after_four_seconds() {
        let (mut c, shared) = setup();
        *shared.start_error.borrow_mut() = Some(CaptureError::NoDevice);
        press(&mut c, 0.0);
        c.poll(3.9);
        assert_eq!(c.view().phase, Phase::Failed);
        c.poll(4.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn new_start_from_failed_works() {
        let (mut c, shared) = setup();
        *shared.start_error.borrow_mut() = Some(CaptureError::NoDevice);
        press(&mut c, 0.0);
        release(&mut c, 0.1);
        *shared.start_error.borrow_mut() = None;
        press(&mut c, 1.0);
        assert_eq!(c.view().phase, Phase::Listening);
    }

    #[test]
    fn levels_are_ignored_while_not_recording() {
        let (mut c, _shared) = setup();
        c.on_level(1.0);
        assert!(c.view().levels.iter().all(|l| *l == 0.0));
    }

    #[test]
    fn levels_are_smoothed_while_recording_and_reset_after() {
        let (mut c, _shared) = setup();
        press(&mut c, 0.0);
        c.on_level(1.0);
        let levels = c.view().levels;
        assert_eq!(levels.len(), 32);
        assert!((levels[31] - 0.4).abs() < 1e-6);
        release(&mut c, 1.0);
        assert!(c.view().levels.iter().all(|l| *l == 0.0));
    }

    #[test]
    fn view_phase_matches_state() {
        let (mut c, shared) = setup();
        assert_eq!(c.view().phase, Phase::Idle);
        press(&mut c, 0.0);
        let v = c.view();
        assert_eq!((v.phase, v.hands_free), (Phase::Listening, false));
        release(&mut c, 1.0);
        *shared.start_error.borrow_mut() = Some(CaptureError::AccessDenied);
        press(&mut c, 2.0);
        assert_eq!(c.view().phase, Phase::Failed);
    }

    #[test]
    fn classifier_recognises_access_denied_in_any_case() {
        for t in [
            "Error 0x80070005",
            "E_ACCESSDENIED",
            "Access Is Denied.",
            "access is denied",
        ] {
            assert_eq!(classify_error(t), CaptureError::AccessDenied, "{t}");
        }
    }

    #[test]
    fn classifier_recognises_no_device_and_other() {
        assert_eq!(
            classify_error("no default input device"),
            CaptureError::NoDevice
        );
        assert_eq!(
            classify_error("stream broke"),
            CaptureError::Other("stream broke".into())
        );
    }
}
