//! Dictation session for the Windows port: key events -> gesture -> recorder, per
//! docs/spec/hotkey.md:43-56, then the recognition states of DictationSession.swift:140-185 and
//! 218-221. The controller owns no engine: it hands out a `Job` and takes the result back. No paste.

use crate::audio::{is_long_enough, smooth_level, LevelHistory};
use crate::context::TargetContext;
use crate::gesture::{HotkeyGesture, Output};
use crate::insert::{clipboard_only_message, InsertOutcome};
use crate::keys::{KeyEvent, KeyFilter, RawKey};
use crate::speech::{done_preview, has_enough_speech, is_junk};

/// Failed state resets to idle after 4 s (DictationSession.swift:234).
pub const FAILED_RESET_SECS: f64 = 4.0;

/// Done state resets to idle after 2.5 s (DictationSession.swift:221).
pub const DONE_RESET_SECS: f64 = 2.5;

/// Shown for silence, an empty transcript and junk (DictationSession.swift:178, 194).
const NOTHING_RECOGNIZED: &str = "Nothing recognized";

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
    Recording {
        hands_free: bool,
    },
    /// Waiting for the model to download or load (DictationSession.swift:11).
    LoadingModel,
    Transcribing,
    /// Pasting the accepted text (DictationSession.swift:9). Busy like transcribing.
    Inserting,
    Done {
        preview: String,
    },
    Failed {
        message: String,
    },
}

impl State {
    /// DictationSession.swift:23-28, without the formatting and inserting states Windows lacks.
    fn is_busy(&self) -> bool {
        matches!(
            self,
            State::LoadingModel | State::Transcribing | State::Inserting
        )
    }
}

/// The recognizer's state as the controller sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelStatus {
    /// Fraction 0..=1 of the download.
    Downloading(f32),
    Loading,
    Ready,
    Failed,
}

/// Recorded audio for the recognizer. `id` tells a late result from the current one.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: u64,
    pub samples: Vec<f32>,
}

/// The accepted text, handed to the insert worker. `captured_password` is the flag read when
/// recording started (`None` when unreadable or when the context never arrived).
#[derive(Debug, Clone, PartialEq)]
pub struct InsertJob {
    pub id: u64,
    pub text: String,
    pub captured_password: Option<bool>,
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
        // A press while busy is ignored (DictationSession.swift:67-70).
        (s, Event::PressBegan) if s.is_busy() => Action::None,
        (_, Event::PressBegan) => Action::Start,
        (State::Recording { .. }, Event::PressEnded) => Action::Stop,
        (State::Recording { hands_free: false }, Event::ShortTap) => Action::Cancel,
        (State::Recording { hands_free: false }, Event::DoubleTap) => Action::MakeHandsFree,
        (State::Recording { .. }, Event::Esc) => Action::Cancel,
        (s, Event::Esc) if s.is_busy() => Action::Cancel,
        _ => Action::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Listening,
    /// Loading the model or transcribing; `View::stage` says which.
    Processing,
    Done,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Loading,
    Transcribing,
    Inserting,
}

/// What the widget shows. `levels` always has 32 entries, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub phase: Phase,
    pub hands_free: bool,
    /// The failure text, or the done preview.
    pub message: String,
    pub levels: Vec<f32>,
    /// Set in `Phase::Processing` only.
    pub stage: Option<Stage>,
    /// Download fraction while the model downloads and a dictation waits for it.
    pub progress: Option<f32>,
}

pub struct Controller<R: Recorder> {
    recorder: R,
    filter: KeyFilter,
    gesture: HotkeyGesture,
    state: State,
    failed_at: f64,
    done_at: f64,
    model: ModelStatus,
    next_job_id: u64,
    /// The job whose result is still wanted; cleared by Esc so a late result is dropped.
    current_job: Option<u64>,
    pending_job: Option<Job>,
    /// The live dictation: set when a recording opens, carried by the capture, the job and the
    /// insert; cleared by Esc so late answers are dropped.
    live: Option<u64>,
    capture: Option<u64>,
    context: Option<TargetContext>,
    pending_insert: Option<InsertJob>,
    insert_preview: String,
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
            done_at: 0.0,
            model: ModelStatus::Loading,
            next_job_id: 1,
            current_job: None,
            pending_job: None,
            live: None,
            capture: None,
            context: None,
            pending_insert: None,
            insert_preview: String::new(),
            smoothed: 0.0,
            history: LevelHistory::new(),
        }
    }

    /// Job ids start at `first`. The app adds an offset per controller restart so a result from
    /// the previous controller never matches a new job.
    pub fn with_first_job_id(mut self, first: u64) -> Self {
        self.next_job_id = first;
        self
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
                    self.begin_processing(samples, at);
                }
            }
            Action::Cancel => self.cancel(),
            Action::MakeHandsFree => self.state = State::Recording { hands_free: true },
        }
    }

    /// Esc. Only a recording owns the microphone (DictationSession.swift:121); a busy state
    /// just forgets its job so the late result is ignored.
    fn cancel(&mut self) {
        if matches!(self.state, State::Recording { .. }) {
            self.stop_recorder();
        }
        self.state = State::Idle;
        self.current_job = None;
        self.pending_job = None;
        self.live = None;
        self.capture = None;
        self.pending_insert = None;
    }

    /// The speech gate, then a job for the recognizer (DictationSession.swift:176-190).
    fn begin_processing(&mut self, samples: Vec<f32>, at: f64) {
        if !has_enough_speech(&samples) {
            self.fail(NOTHING_RECOGNIZED.to_string(), at);
            return;
        }
        // The dictation id doubles as the job id; a start that never opened has none to reuse.
        let id = self.live.unwrap_or_else(|| self.alloc_id());
        self.current_job = Some(id);
        self.pending_job = Some(Job { id, samples });
        self.state = if self.model == ModelStatus::Ready {
            State::Transcribing
        } else {
            State::LoadingModel
        };
    }

    fn fail(&mut self, message: String, at: f64) {
        self.state = State::Failed { message };
        self.failed_at = at;
    }

    /// The model changed state. A dictation waiting in `LoadingModel` moves on once it is ready.
    pub fn on_model(&mut self, status: ModelStatus) {
        self.model = status;
        if status == ModelStatus::Ready && self.state == State::LoadingModel {
            self.state = State::Transcribing;
        }
    }

    /// The recognizer finished job `id`. A result for any other id is dropped.
    pub fn on_recognized(&mut self, id: u64, result: Result<String, String>, at: f64) {
        if self.current_job != Some(id) {
            return;
        }
        self.current_job = None;
        match result {
            // Empty and junk transcripts fail the same way (DictationSession.swift:194).
            Ok(text) if text.trim().is_empty() || is_junk(&text) => {
                self.fail(NOTHING_RECOGNIZED.to_string(), at)
            }
            Ok(text) => {
                self.insert_preview = done_preview(&text);
                self.pending_insert = Some(InsertJob {
                    id,
                    text,
                    captured_password: self.context.as_ref().and_then(|c| c.password),
                });
                self.state = State::Inserting;
            }
            Err(message) => self.fail(message, at),
        }
    }

    /// The job created by the last stop, handed over once.
    pub fn take_job(&mut self) -> Option<Job> {
        self.pending_job.take()
    }

    /// The id the recognizer should still work on, if any.
    pub fn current_job(&self) -> Option<u64> {
        self.current_job
    }

    /// The dictation id of the latest start, handed over once: the app captures the target context
    /// for it on a worker. Only a start whose recorder opened gives one.
    pub fn take_capture(&mut self) -> Option<u64> {
        self.capture.take()
    }

    /// The captured context for dictation `id`. Dropped unless `id` is the live dictation.
    pub fn on_context(&mut self, id: u64, context: TargetContext) {
        if self.live == Some(id) {
            self.context = Some(context);
        }
    }

    /// The context stored for the live dictation, if it has arrived.
    pub fn context(&self) -> Option<&TargetContext> {
        self.context.as_ref()
    }

    /// The accepted transcript as an insert job, handed over once. Replaces `take_result`.
    pub fn take_insert(&mut self) -> Option<InsertJob> {
        self.pending_insert.take()
    }

    /// The insert worker finished dictation `id`. An id that is not the live one is dropped. Ok
    /// shows Done (clipboard-only with the Ctrl+V hint); Err fails with its message.
    pub fn on_inserted(&mut self, id: u64, result: Result<InsertOutcome, String>, at: f64) {
        if self.live != Some(id) || self.state != State::Inserting {
            return;
        }
        match result {
            Ok(InsertOutcome::Pasted) => {
                self.state = State::Done {
                    preview: self.insert_preview.clone(),
                };
                self.done_at = at;
            }
            Ok(InsertOutcome::ClipboardOnly) => {
                self.state = State::Done {
                    preview: clipboard_only_message(&self.insert_preview),
                };
                self.done_at = at;
            }
            Err(message) => self.fail(message, at),
        }
    }

    fn start(&mut self, at: f64) {
        self.pending_insert = None;
        self.live = None;
        self.capture = None;
        self.context = None;
        self.clear_levels();
        match self.recorder.start() {
            Ok(()) => {
                let id = self.alloc_id();
                self.live = Some(id);
                self.capture = Some(id);
                self.state = State::Recording { hands_free: false };
            }
            Err(e) => self.fail(format!("Microphone: {e}"), at),
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_job_id;
        self.next_job_id += 1;
        id
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

    /// The capture stream reported an error. Only matters while recording: the microphone is
    /// released and the widget shows the failure, same as a failed start.
    pub fn on_stream_error(&mut self, error: CaptureError, at: f64) {
        if !matches!(self.state, State::Recording { .. }) {
            return;
        }
        self.stop_recorder();
        self.fail(format!("Microphone: {error}"), at);
    }

    /// Resets Done and Failed to idle once their time has passed.
    pub fn poll(&mut self, now: f64) {
        let expired = match self.state {
            State::Failed { .. } => now - self.failed_at >= FAILED_RESET_SECS,
            State::Done { .. } => now - self.done_at >= DONE_RESET_SECS,
            _ => false,
        };
        if expired {
            self.state = State::Idle;
        }
    }

    pub fn view(&self) -> View {
        let (phase, hands_free, message, stage) = match &self.state {
            State::Idle => (Phase::Idle, false, String::new(), None),
            State::Recording { hands_free } => (Phase::Listening, *hands_free, String::new(), None),
            State::LoadingModel => (
                Phase::Processing,
                false,
                String::new(),
                Some(Stage::Loading),
            ),
            State::Transcribing => (
                Phase::Processing,
                false,
                String::new(),
                Some(Stage::Transcribing),
            ),
            State::Inserting => (
                Phase::Processing,
                false,
                String::new(),
                Some(Stage::Inserting),
            ),
            State::Done { preview } => (Phase::Done, false, preview.clone(), None),
            State::Failed { message } => (Phase::Failed, false, message.clone(), None),
        };
        let progress = match (&self.state, self.model) {
            (State::LoadingModel, ModelStatus::Downloading(p)) => Some(p),
            _ => None,
        };
        View {
            phase,
            hands_free,
            message,
            levels: self.history.levels().to_vec(),
            stage,
            progress,
        }
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
        silent: Cell<bool>,
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
            // Half silence, half tone: a constant 0.1 has no frame above the noise floor and
            // would fail the speech gate.
            let n = self.0.stop_len.get();
            if self.0.silent.get() {
                return vec![0.0; n];
            }
            let mut v = vec![0.0; n / 2];
            v.extend(vec![0.1; n - n / 2]);
            v
        }
    }

    fn setup() -> (Controller<FakeRecorder>, Rc<Shared>) {
        let shared = Rc::new(Shared::default());
        shared.stop_len.set(16_000);
        (Controller::new(FakeRecorder(shared.clone())), shared)
    }

    fn setup_ready() -> (Controller<FakeRecorder>, Rc<Shared>) {
        let (mut c, shared) = setup();
        c.on_model(ModelStatus::Ready);
        (c, shared)
    }

    fn dictate(c: &mut Controller<FakeRecorder>, from: f64) {
        press(c, from);
        release(c, from + 1.0);
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
        let (mut c, shared) = setup_ready();
        press(&mut c, 0.0);
        assert_eq!(c.view().phase, Phase::Listening);
        release(&mut c, 1.0);
        assert_eq!(c.view().phase, Phase::Processing);
        assert_eq!(shared.stops.get(), 1);
        assert_eq!(c.take_job().map(|j| j.samples.len()), Some(16_000));
        assert_eq!(c.take_job(), None);
    }

    #[test]
    fn recording_under_0_4_seconds_is_dropped() {
        let (mut c, shared) = setup();
        shared.stop_len.set(6399);
        press(&mut c, 0.0);
        release(&mut c, 1.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_job(), None);
    }

    #[test]
    fn recording_of_exactly_0_4_seconds_is_kept() {
        let (mut c, shared) = setup();
        shared.stop_len.set(6400);
        press(&mut c, 0.0);
        release(&mut c, 1.0);
        // Past the 0.4 s length check; the fake's 0.2 s of tone then fails the 0.3 s speech gate.
        let v = c.view();
        assert_eq!(
            (v.phase, v.message.as_str()),
            (Phase::Failed, "Nothing recognized")
        );
    }

    #[test]
    fn short_tap_starts_then_stops_and_keeps_nothing() {
        let (mut c, shared) = setup();
        press(&mut c, 0.0);
        release(&mut c, 0.1);
        assert_eq!((shared.starts.get(), shared.stops.get()), (1, 1));
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_job(), None);
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
        assert_eq!(c.view().phase, Phase::Processing);
        assert_eq!(c.take_job().map(|j| j.samples.len()), Some(16_000));
    }

    #[test]
    fn esc_while_recording_cancels_without_capture() {
        let (mut c, shared) = setup();
        press(&mut c, 0.0);
        esc(&mut c, 0.5);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(shared.stops.get(), 1);
        release(&mut c, 1.0);
        assert_eq!(c.take_job(), None);
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
        assert_eq!(c.take_job().map(|j| j.samples.len()), Some(16_000));
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
    fn stream_error_while_recording_fails_then_resets() {
        let (mut c, shared) = setup();
        press(&mut c, 0.0);
        c.on_stream_error(CaptureError::Other("device lost".into()), 2.0);
        let v = c.view();
        assert_eq!(v.phase, Phase::Failed);
        assert_eq!(v.message, "Microphone: device lost");
        assert_eq!(shared.stops.get(), 1);
        assert!(c.take_job().is_none());
        c.poll(5.9);
        assert_eq!(c.view().phase, Phase::Failed);
        c.poll(6.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn stream_error_while_idle_is_ignored() {
        let (mut c, _shared) = setup();
        c.on_stream_error(CaptureError::NoDevice, 1.0);
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
        assert_eq!(c.view().phase, Phase::Processing);
        esc(&mut c, 1.5);
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

    // The recognition states.

    #[test]
    fn press_began_is_ignored_while_busy_and_starts_from_done() {
        for s in [State::LoadingModel, State::Transcribing, State::Inserting] {
            assert_eq!(action_for(&s, Event::PressBegan), Action::None);
        }
        let done = State::Done {
            preview: "x".into(),
        };
        assert_eq!(action_for(&done, Event::PressBegan), Action::Start);
    }

    #[test]
    fn esc_cancels_while_busy_and_is_ignored_in_done_and_failed() {
        assert_eq!(action_for(&State::LoadingModel, Event::Esc), Action::Cancel);
        assert_eq!(action_for(&State::Transcribing, Event::Esc), Action::Cancel);
        assert_eq!(action_for(&State::Inserting, Event::Esc), Action::Cancel);
        let done = State::Done {
            preview: "x".into(),
        };
        let failed = State::Failed {
            message: "x".into(),
        };
        assert_eq!(action_for(&done, Event::Esc), Action::None);
        assert_eq!(action_for(&failed, Event::Esc), Action::None);
    }

    #[test]
    fn stop_with_speech_and_a_ready_model_transcribes_and_queues_a_job() {
        let (mut c, _s) = setup_ready();
        dictate(&mut c, 0.0);
        let v = c.view();
        assert_eq!(
            (v.phase, v.stage),
            (Phase::Processing, Some(Stage::Transcribing))
        );
        let job = c.take_job().expect("job");
        assert_eq!(job.samples.len(), 16_000);
        assert_eq!(c.current_job(), Some(job.id));
    }

    #[test]
    fn stop_before_the_model_is_ready_waits_in_loading_then_transcribes() {
        let (mut c, _s) = setup();
        c.on_model(ModelStatus::Loading);
        dictate(&mut c, 0.0);
        let v = c.view();
        assert_eq!(
            (v.phase, v.stage),
            (Phase::Processing, Some(Stage::Loading))
        );
        assert!(c.take_job().is_some());
        c.on_model(ModelStatus::Ready);
        assert_eq!(c.view().stage, Some(Stage::Transcribing));
    }

    #[test]
    fn silence_fails_with_nothing_recognized_and_makes_no_job() {
        let (mut c, shared) = setup_ready();
        shared.silent.set(true);
        dictate(&mut c, 0.0);
        let v = c.view();
        assert_eq!(
            (v.phase, v.message.as_str()),
            (Phase::Failed, "Nothing recognized")
        );
        assert_eq!(c.take_job(), None);
        c.poll(4.9);
        assert_eq!(c.view().phase, Phase::Failed);
        c.poll(5.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    fn transcribe(c: &mut Controller<FakeRecorder>, at: f64) -> u64 {
        dictate(c, at);
        c.take_job().expect("job").id
    }

    #[test]
    fn ok_result_is_done_for_two_and_a_half_seconds() {
        let (mut c, _s) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        c.on_recognized(id, Ok("hello there".into()), 10.0);
        let v = c.view();
        // Accepted text goes through Inserting; Done starts when the worker reports back.
        assert_eq!(
            (v.phase, v.stage),
            (Phase::Processing, Some(Stage::Inserting))
        );
        let job = c.take_insert().expect("insert job");
        assert_eq!((job.id, job.text.as_str()), (id, "hello there"));
        assert_eq!(c.take_insert(), None);
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 10.5);
        let v = c.view();
        assert_eq!((v.phase, v.message.as_str()), (Phase::Done, "hello there"));
        c.poll(12.99);
        assert_eq!(c.view().phase, Phase::Done);
        c.poll(13.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn blank_or_junk_results_are_nothing_recognized() {
        for text in ["   \n", "", "Thank you."] {
            let (mut c, _s) = setup_ready();
            let id = transcribe(&mut c, 0.0);
            c.on_recognized(id, Ok(text.into()), 5.0);
            let v = c.view();
            assert_eq!(
                (v.phase, v.message.as_str()),
                (Phase::Failed, "Nothing recognized"),
                "{text:?}"
            );
            assert_eq!(c.take_insert(), None);
        }
    }

    #[test]
    fn an_error_result_fails_with_its_message() {
        let (mut c, _s) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        c.on_recognized(id, Err("Transcription failed: boom".into()), 5.0);
        let v = c.view();
        assert_eq!(
            (v.phase, v.message.as_str()),
            (Phase::Failed, "Transcription failed: boom")
        );
    }

    #[test]
    fn esc_while_transcribing_goes_idle_at_once_and_a_late_result_is_ignored() {
        let (mut c, shared) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        esc(&mut c, 2.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(shared.stops.get(), 1);
        assert_eq!(c.current_job(), None);
        c.on_recognized(id, Ok("late words".into()), 3.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_insert(), None);
    }

    #[test]
    fn late_result_after_esc_is_ignored() {
        let (mut c, _s) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        esc(&mut c, 2.0);
        c.on_recognized(id, Err("x".into()), 3.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn esc_while_loading_then_ready_stays_idle() {
        let (mut c, _s) = setup();
        transcribe(&mut c, 0.0);
        assert_eq!(c.view().stage, Some(Stage::Loading));
        esc(&mut c, 2.0);
        c.on_model(ModelStatus::Ready);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn a_result_with_an_old_id_is_ignored_while_a_newer_job_runs() {
        let (mut c, _s) = setup_ready();
        let old = transcribe(&mut c, 0.0);
        esc(&mut c, 1.5);
        let new = transcribe(&mut c, 3.0);
        assert_ne!(old, new);
        c.on_recognized(old, Ok("stale".into()), 5.0);
        assert_eq!(c.view().phase, Phase::Processing);
        assert_eq!(c.current_job(), Some(new));
        c.on_recognized(new, Ok("fresh".into()), 6.0);
        assert_eq!(c.take_insert().map(|j| j.text), Some("fresh".to_string()));
        c.on_inserted(new, Ok(InsertOutcome::Pasted), 6.5);
        assert_eq!(c.view().message, "fresh");
    }

    #[test]
    fn press_while_transcribing_does_not_start_the_recorder() {
        let (mut c, shared) = setup_ready();
        transcribe(&mut c, 0.0);
        press(&mut c, 2.0);
        assert_eq!(shared.starts.get(), 1);
        assert_eq!(c.view().phase, Phase::Processing);
    }

    #[test]
    fn a_new_dictation_can_start_from_done() {
        let (mut c, shared) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        c.on_recognized(id, Ok("hello".into()), 2.0);
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 2.5);
        press(&mut c, 3.0);
        assert_eq!(c.view().phase, Phase::Listening);
        assert_eq!(shared.starts.get(), 2);
    }

    #[test]
    fn download_progress_shows_while_waiting_and_ready_clears_it() {
        let (mut c, _s) = setup();
        c.on_model(ModelStatus::Downloading(0.0));
        dictate(&mut c, 0.0);
        c.on_model(ModelStatus::Downloading(0.42));
        let v = c.view();
        assert_eq!((v.stage, v.progress), (Some(Stage::Loading), Some(0.42)));
        c.on_model(ModelStatus::Ready);
        let v = c.view();
        assert_eq!((v.stage, v.progress), (Some(Stage::Transcribing), None));
    }

    #[test]
    fn first_job_id_can_be_set() {
        let shared = Rc::new(Shared::default());
        shared.stop_len.set(16_000);
        let mut c = Controller::new(FakeRecorder(shared)).with_first_job_id(1 << 32);
        c.on_model(ModelStatus::Ready);
        assert_eq!(transcribe(&mut c, 0.0), 1 << 32);
        assert_eq!(transcribe_after_done(&mut c), (1 << 32) + 1);
    }

    fn transcribe_after_done(c: &mut Controller<FakeRecorder>) -> u64 {
        let id = c.current_job().unwrap();
        c.on_recognized(id, Err("x".into()), 3.0);
        transcribe(c, 10.0)
    }

    // Insertion.

    fn start_dictation(c: &mut Controller<FakeRecorder>, at: f64) -> u64 {
        press(c, at);
        let id = c.take_capture().expect("capture id");
        release(c, at + 1.0);
        id
    }

    fn inserting(c: &mut Controller<FakeRecorder>, text: &str) -> u64 {
        let id = start_dictation(c, 0.0);
        let job = c.take_job().expect("job");
        assert_eq!(job.id, id);
        c.on_recognized(id, Ok(text.into()), 2.0);
        id
    }

    #[test]
    fn inserting_is_busy_and_shows_the_inserting_stage() {
        let (mut c, shared) = setup_ready();
        inserting(&mut c, "hello");
        let v = c.view();
        assert_eq!(
            (v.phase, v.stage),
            (Phase::Processing, Some(Stage::Inserting))
        );
        press(&mut c, 3.0);
        assert_eq!(shared.starts.get(), 1);
        assert_eq!(c.view().stage, Some(Stage::Inserting));
    }

    #[test]
    fn each_start_gives_one_capture_with_the_dictation_id() {
        let (mut c, _s) = setup_ready();
        assert_eq!(c.take_capture(), None);
        press(&mut c, 0.0);
        let id = c.take_capture().expect("capture");
        assert_eq!(c.take_capture(), None);
        release(&mut c, 1.0);
        assert_eq!(c.take_job().map(|j| j.id), Some(id));
    }

    #[test]
    fn a_failed_start_gives_no_capture() {
        let (mut c, shared) = setup_ready();
        *shared.start_error.borrow_mut() = Some(CaptureError::NoDevice);
        press(&mut c, 0.0);
        assert_eq!(c.take_capture(), None);
    }

    #[test]
    fn a_second_start_gets_a_new_dictation_id() {
        let (mut c, _s) = setup_ready();
        let first = start_dictation(&mut c, 0.0);
        let job = c.take_job().unwrap();
        c.on_recognized(job.id, Err("x".into()), 2.0);
        let second = start_dictation(&mut c, 10.0);
        assert_ne!(first, second);
    }

    #[test]
    fn the_context_is_stored_for_the_live_dictation() {
        let (mut c, _s) = setup_ready();
        press(&mut c, 0.0);
        let id = c.take_capture().unwrap();
        assert_eq!(c.context(), None);
        let ctx = TargetContext {
            app_id: Some("notepad.exe".into()),
            ..TargetContext::default()
        };
        c.on_context(id, ctx.clone());
        assert_eq!(c.context(), Some(&ctx));
    }

    #[test]
    fn a_context_for_a_stale_id_is_dropped() {
        let (mut c, _s) = setup_ready();
        press(&mut c, 0.0);
        let old = c.take_capture().unwrap();
        release(&mut c, 0.1);
        press(&mut c, 1.0);
        let new = c.take_capture().unwrap();
        assert_ne!(old, new);
        c.on_context(
            old,
            TargetContext {
                app_id: Some("old.exe".into()),
                ..Default::default()
            },
        );
        assert_eq!(c.context(), None);
        c.on_context(
            new,
            TargetContext {
                app_id: Some("new.exe".into()),
                ..Default::default()
            },
        );
        assert_eq!(
            c.context().and_then(|x| x.app_id.as_deref()),
            Some("new.exe")
        );
    }

    #[test]
    fn a_context_after_esc_is_dropped() {
        let (mut c, _s) = setup_ready();
        press(&mut c, 0.0);
        let id = c.take_capture().unwrap();
        esc(&mut c, 0.5);
        c.on_context(id, TargetContext::default());
        assert_eq!(c.context(), None);
    }

    #[test]
    fn the_insert_job_carries_the_captured_password_flag() {
        for flag in [Some(true), Some(false), None] {
            let (mut c, _s) = setup_ready();
            press(&mut c, 0.0);
            let id = c.take_capture().unwrap();
            c.on_context(
                id,
                TargetContext {
                    password: flag,
                    ..Default::default()
                },
            );
            release(&mut c, 1.0);
            c.take_job().unwrap();
            c.on_recognized(id, Ok("hello".into()), 2.0);
            let job = c.take_insert().unwrap();
            assert_eq!((job.id, job.captured_password), (id, flag), "{flag:?}");
        }
    }

    #[test]
    fn without_a_context_the_captured_flag_is_unknown() {
        let (mut c, _s) = setup_ready();
        inserting(&mut c, "hello");
        assert_eq!(c.take_insert().unwrap().captured_password, None);
    }

    #[test]
    fn esc_while_inserting_goes_idle_and_the_late_outcome_is_ignored() {
        let (mut c, _s) = setup_ready();
        let id = inserting(&mut c, "hello");
        esc(&mut c, 2.5);
        assert_eq!(c.view().phase, Phase::Idle);
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
        assert_eq!(c.view().phase, Phase::Idle);
        c.on_inserted(id, Err("late".into()), 3.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn esc_while_inserting_drops_an_insert_job_not_yet_taken() {
        let (mut c, _s) = setup_ready();
        inserting(&mut c, "hello");
        esc(&mut c, 2.5);
        assert_eq!(c.take_insert(), None);
    }

    #[test]
    fn an_outcome_for_another_id_is_ignored() {
        let (mut c, _s) = setup_ready();
        let id = inserting(&mut c, "hello");
        c.on_inserted(id + 1, Ok(InsertOutcome::Pasted), 3.0);
        assert_eq!(c.view().stage, Some(Stage::Inserting));
    }

    #[test]
    fn an_insert_error_fails_with_its_message_and_resets_after_four_seconds() {
        let (mut c, _s) = setup_ready();
        let id = inserting(&mut c, "hello");
        c.on_inserted(
            id,
            Err("A password field is active; nothing was inserted".into()),
            3.0,
        );
        let v = c.view();
        assert_eq!(
            (v.phase, v.message.as_str()),
            (
                Phase::Failed,
                "A password field is active; nothing was inserted"
            )
        );
        c.poll(6.9);
        assert_eq!(c.view().phase, Phase::Failed);
        c.poll(7.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn clipboard_only_shows_the_ctrl_v_hint_before_the_preview() {
        let (mut c, _s) = setup_ready();
        let id = inserting(&mut c, "line one\nline two");
        c.on_inserted(id, Ok(InsertOutcome::ClipboardOnly), 3.0);
        let v = c.view();
        assert_eq!(v.phase, Phase::Done);
        assert_eq!(
            v.message,
            "Copied (press Ctrl+V to paste): line one line two"
        );
    }

    #[test]
    fn done_after_insertion_resets_two_and_a_half_seconds_after_the_outcome() {
        let (mut c, _s) = setup_ready();
        let id = inserting(&mut c, "hello");
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 20.0);
        c.poll(22.4);
        assert_eq!(c.view().phase, Phase::Done);
        c.poll(22.5);
        assert_eq!(c.view().phase, Phase::Idle);
    }
}
