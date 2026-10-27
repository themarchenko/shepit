//! Dictation session for the Windows port: key events -> gesture -> recorder, per
//! docs/spec/hotkey.md:43-56, then the recognition states of DictationSession.swift:140-185 and
//! 218-221. The controller owns no engine: it hands out a `Job` and takes the result back. No paste.

use crate::audio::{is_digital_silence, is_long_enough, smooth_level, LevelHistory};
use crate::context::TargetContext;
use crate::gesture::{HotkeyGesture, Output};
use crate::history::{insert_method_id, status_id};
use crate::insert::{clipboard_only_message, InsertOutcome, PASSWORD_REFUSED};
use crate::keys::{Hotkey, KeyEvent, KeyFilter, RawKey};
use crate::live::{Config as LiveConfig, LivePreview, LiveSource, LiveText};
use crate::polish::route::Source;
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
    /// The device opened but delivered only digital silence (muted, or blocked by Windows).
    Silent,
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
            CaptureError::Silent => {
                write!(f, "no sound, check Windows microphone privacy settings")
            }
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

/// `LiveSource` has no default methods: a recorder that cannot be read while it runs has no business here.
pub trait Recorder: LiveSource {
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
    /// Polishing the accepted transcript: any route, from the accepted transcript until the text
    /// is ready (DictationSession.swift:188). Busy like transcribing.
    Formatting,
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
    /// DictationSession.swift:23-28.
    fn is_busy(&self) -> bool {
        matches!(
            self,
            State::LoadingModel | State::Transcribing | State::Formatting | State::Inserting
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

/// Audio for one live preview pass. `id` is the dictation id and `seq` the pass, so a late
/// answer is dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewJob {
    pub id: u64,
    pub seq: u64,
    pub samples: Vec<f32>,
}

/// The accepted transcript, handed to the polish worker. `context` is the captured target
/// context, `None` when it never arrived. The id is the dictation id, so a late result is dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatJob {
    pub id: u64,
    pub transcript: String,
    pub context: Option<TargetContext>,
}

/// The final text, handed to the insert worker. `captured_password` is the flag read when
/// recording started (`None` when unreadable or when the context never arrived).
#[derive(Debug, Clone, PartialEq)]
pub struct InsertJob {
    pub id: u64,
    pub text: String,
    pub captured_password: Option<bool>,
}

/// What the polish worker knows about a finished format, beyond the text: the profile and mode it
/// resolved, the result's source and language, the terms it reports as fixed and how long the
/// agent took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatMeta {
    pub profile: String,
    pub mode: String,
    pub source: Source,
    pub language: Option<String>,
    pub new_terms: Vec<String>,
    pub llm_ms: u64,
}

/// Every column of a history row except `id`, `createdAt` (the store stamps it), `finalText`
/// (never written, as on macOS) and `audioFile` (known after the WAV is saved).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordDraft {
    /// The exe name (macOS `bundleID`).
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub profile: String,
    pub mode: String,
    pub engine: String,
    pub raw_text: String,
    pub formatted_text: String,
    pub language: Option<String>,
    pub audio_seconds: f64,
    pub stt_ms: u64,
    pub llm_ms: u64,
    pub formatter_source: String,
    pub insert_method: String,
    pub status: String,
}

/// How one dictation that passed the 0.4 s length check ended. `audio` is what to save as a WAV,
/// `record` the history row. A password field gives neither.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub id: u64,
    pub audio: Option<Vec<f32>>,
    pub record: Option<RecordDraft>,
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
    Formatting,
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
    /// The app the text goes to (`TargetContext::app_name`), once the context has arrived.
    pub target: Option<String>,
    /// The words heard so far. Display only; it stays through processing and is cleared when
    /// the dictation ends.
    pub live: LiveText,
}

/// Input from the widget itself (WidgetController.swift:51-53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetInput {
    /// Toggles hands-free.
    Click,
    /// Push-to-talk starts.
    HoldBegan,
    /// Push-to-talk stops.
    HoldEnded,
}

/// The `engine` column; Windows has one recognizer.
const ENGINE_ID: &str = "parakeet-v3";

/// What the live dictation keeps from the moment its audio is accepted for processing until it
/// ends. The audio stays here (not in the job) so a failed dictation can still be saved.
struct Run {
    id: u64,
    samples: Vec<f32>,
    raw_text: String,
    stt_ms: u64,
    /// The final text and what the polish worker reported, once formatted with `on_formatted_with`.
    formatted: Option<(String, FormatMeta)>,
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
    pending_format: Option<FormatJob>,
    pending_insert: Option<InsertJob>,
    insert_preview: String,
    smoothed: f32,
    history: LevelHistory,
    run: Option<Run>,
    finished: Option<Finished>,
    new_terms: Vec<String>,
    preview: LivePreview,
    pending_preview: Option<PreviewJob>,
    /// The setting as last pushed by the app; copied into the run at start.
    live_setting: bool,
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
            pending_format: None,
            pending_insert: None,
            insert_preview: String::new(),
            smoothed: 0.0,
            history: LevelHistory::new(),
            run: None,
            finished: None,
            new_terms: Vec::new(),
            preview: LivePreview::new(LiveConfig::default()),
            pending_preview: None,
            live_setting: true,
        }
    }

    /// Job ids start at `first`. The app adds an offset per controller restart so a result from
    /// the previous controller never matches a new job.
    pub fn with_first_job_id(mut self, first: u64) -> Self {
        self.next_job_id = first;
        self
    }

    /// Starts on `hotkey` instead of Right Ctrl.
    pub fn with_hotkey(mut self, hotkey: Hotkey) -> Self {
        self.set_hotkey(hotkey);
        self
    }

    /// Switches the key. A new filter and gesture drop a half-finished press of the old key.
    pub fn set_hotkey(&mut self, hotkey: Hotkey) {
        self.filter = KeyFilter::for_hotkey(hotkey);
        self.gesture = HotkeyGesture::new();
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
            Action::Start => self.start(at, false),
            Action::Stop => {
                self.stop_and_process(at);
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
        self.pending_format = None;
        self.pending_insert = None;
        self.new_terms.clear();
        self.clear_preview();
        self.finish(None, false);
    }

    /// Ends the current run, if any, into `finished`. `inserted` is the outcome when the text
    /// reached the target; without it only the audio is kept. The flag is the one captured when
    /// recording started. After an insert, a password field keeps neither audio nor row, and
    /// neither does one whose insert was refused for that reason. Without an insert the audio is
    /// kept only when the flag says the field was clear: an unread or unreadable flag (None) is
    /// not proof, and an early end such as silence or Esc can come before the flag is read.
    fn finish(&mut self, inserted: Option<InsertOutcome>, refused: bool) {
        let Some(run) = self.run.take() else { return };
        let flag = self.context.as_ref().and_then(|c| c.password);
        if refused || flag == Some(true) || (inserted.is_none() && flag != Some(false)) {
            return;
        }
        let record = match (inserted, run.formatted) {
            (Some(outcome), Some((formatted_text, meta))) => {
                let ctx = self.context.as_ref();
                Some(RecordDraft {
                    app_id: ctx.and_then(|c| c.app_id.clone()),
                    app_name: ctx.and_then(|c| c.app_name.clone()),
                    window_title: ctx.and_then(|c| c.window_title.clone()),
                    profile: meta.profile,
                    mode: meta.mode,
                    engine: ENGINE_ID.to_string(),
                    raw_text: run.raw_text,
                    formatted_text,
                    language: meta.language,
                    audio_seconds: run.samples.len() as f64 / 16_000.0,
                    stt_ms: run.stt_ms,
                    llm_ms: meta.llm_ms,
                    formatter_source: meta.source.as_str().to_string(),
                    insert_method: insert_method_id(outcome).to_string(),
                    status: status_id(outcome).to_string(),
                })
            }
            _ => None,
        };
        self.finished = Some(Finished {
            id: run.id,
            audio: Some(run.samples),
            record,
        });
    }

    /// The speech gate, then a job for the recognizer (DictationSession.swift:176-190).
    fn begin_processing(&mut self, samples: Vec<f32>, at: f64) {
        // The dictation id doubles as the job id; a start that never opened has none to reuse.
        let id = self.live.unwrap_or_else(|| self.alloc_id());
        self.run = Some(Run {
            id,
            samples: samples.clone(),
            raw_text: String::new(),
            stt_ms: 0,
            formatted: None,
        });
        // Exact zeros mean a muted or blocked microphone, not a quiet room, so say that instead of
        // "Nothing recognized". Checked first: the speech gate would swallow it.
        if is_digital_silence(&samples) {
            self.fail(format!("Microphone: {}", CaptureError::Silent), at);
            self.finish(None, false);
            return;
        }
        if !has_enough_speech(&samples) {
            self.fail(NOTHING_RECOGNIZED.to_string(), at);
            self.finish(None, false);
            return;
        }
        self.current_job = Some(id);
        self.pending_job = Some(Job { id, samples });
        self.state = if self.model == ModelStatus::Ready {
            State::Transcribing
        } else {
            State::LoadingModel
        };
    }

    fn fail(&mut self, message: String, at: f64) {
        self.clear_preview();
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
                self.fail(NOTHING_RECOGNIZED.to_string(), at);
                self.finish(None, false);
            }
            Ok(text) => {
                if let Some(run) = self.run.as_mut().filter(|r| r.id == id) {
                    run.raw_text = text.clone();
                }
                self.pending_format = Some(FormatJob {
                    id,
                    transcript: text,
                    context: self.context.clone(),
                });
                self.state = State::Formatting;
            }
            Err(message) => {
                self.fail(message, at);
                self.finish(None, false);
            }
        }
    }

    /// The accepted transcript as a format job, handed over once.
    pub fn take_format(&mut self) -> Option<FormatJob> {
        self.pending_format.take()
    }

    /// The id of the dictation being formatted, if any: the polish worker drops anything else.
    pub fn formatting_id(&self) -> Option<u64> {
        if self.state == State::Formatting {
            self.live
        } else {
            None
        }
    }

    /// The polish worker finished dictation `id` with the final `text`. Dropped unless `id` is
    /// the live dictation and the state is Formatting. Moves to Inserting with an insert job.
    pub fn on_formatted(&mut self, id: u64, text: String, at: f64) {
        let _ = at;
        // The riskiest control of the polishing path: without it a late agent reply could paste
        // cancelled text into whatever app has focus.
        if self.live != Some(id) || self.state != State::Formatting {
            return;
        }
        self.insert_preview = done_preview(&text);
        self.pending_insert = Some(InsertJob {
            id,
            text,
            captured_password: self.context.as_ref().and_then(|c| c.password),
        });
        self.state = State::Inserting;
    }

    /// How long the recognizer took on dictation `id`, in ms. Dropped unless `id` is the live one.
    pub fn on_stt_ms(&mut self, id: u64, ms: u64) {
        if self.live != Some(id) {
            return;
        }
        if let Some(run) = self.run.as_mut().filter(|r| r.id == id) {
            run.stt_ms = ms;
        }
    }

    /// `on_formatted` with what the history needs. The same drop rules apply.
    pub fn on_formatted_with(&mut self, id: u64, text: String, meta: FormatMeta, at: f64) {
        if self.live != Some(id) || self.state != State::Formatting {
            return;
        }
        self.new_terms = meta.new_terms.clone();
        if let Some(run) = self.run.as_mut().filter(|r| r.id == id) {
            run.formatted = Some((text.clone(), meta));
        }
        self.on_formatted(id, text, at);
    }

    /// The end of the last dictation that passed the length check, handed over once. Esc while
    /// recording and a recording that was too short give nothing.
    pub fn take_finished(&mut self) -> Option<Finished> {
        self.finished.take()
    }

    /// The terms the agent reported for the live dictation, handed over once. Esc or a stale id
    /// gives none.
    pub fn take_new_terms(&mut self) -> Vec<String> {
        std::mem::take(&mut self.new_terms)
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
        self.clear_preview();
        match &result {
            Ok(outcome) => self.finish(Some(*outcome), false),
            Err(m) => self.finish(None, m == PASSWORD_REFUSED),
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

    fn start(&mut self, at: f64, hands_free: bool) {
        self.run = None;
        self.new_terms.clear();
        self.pending_format = None;
        self.pending_insert = None;
        self.live = None;
        self.capture = None;
        self.context = None;
        self.clear_preview();
        self.clear_levels();
        match self.recorder.start() {
            Ok(()) => {
                if self.live_setting {
                    self.preview.start(at);
                }
                let id = self.alloc_id();
                self.live = Some(id);
                self.capture = Some(id);
                self.state = State::Recording { hands_free };
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
        // The preview ends before the recorder does, so no pass is handed out for a recording
        // that is over. Its text stays until the dictation ends.
        self.preview.stop();
        self.pending_preview = None;
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
        self.schedule_preview(now);
    }

    fn clear_preview(&mut self) {
        self.preview.clear();
        self.pending_preview = None;
    }

    /// Only a recording with a ready model asks for a pass: a pass handed out earlier would
    /// start the model load for nothing.
    fn schedule_preview(&mut self, now: f64) {
        if !matches!(self.state, State::Recording { .. })
            || self.model != ModelStatus::Ready
            || self.pending_preview.is_some()
        {
            return;
        }
        let (Some(id), Some(req)) = (self.live, self.preview.next(now, &self.recorder)) else {
            return;
        };
        self.pending_preview = Some(PreviewJob {
            id,
            seq: req.seq,
            samples: req.samples,
        });
    }

    /// The app's `livePreview` setting. It is read when a recording starts, so a change applies
    /// to the next dictation.
    pub fn set_live_preview(&mut self, on: bool) {
        self.live_setting = on;
    }

    /// The next preview pass for the recognizer, once.
    pub fn take_preview(&mut self) -> Option<PreviewJob> {
        self.pending_preview.take()
    }

    /// The dictation whose preview passes are still wanted: the recorder is open and the
    /// preview was on at start. The recognizer skips queued passes for any other id.
    pub fn preview_wanted(&self) -> Option<u64> {
        if matches!(self.state, State::Recording { .. }) && self.preview.is_running() {
            self.live
        } else {
            None
        }
    }

    /// A preview pass finished in `secs`. An answer for another dictation, or after the key was
    /// released, is dropped.
    pub fn on_previewed(
        &mut self,
        id: u64,
        seq: u64,
        result: Result<String, String>,
        secs: f64,
        at: f64,
    ) {
        if self.live != Some(id) || !matches!(self.state, State::Recording { .. }) {
            return;
        }
        self.preview.on_result(seq, result, secs, at);
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
            State::Formatting => (
                Phase::Processing,
                false,
                String::new(),
                Some(Stage::Formatting),
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
            target: self.context.as_ref().and_then(|c| c.app_name.clone()),
            live: self.preview.text().clone(),
        }
    }

    /// Input from the widget. A click toggles hands-free, a hold is push-to-talk
    /// (DictationSession.swift:129-136). Busy states ignore both, like a hotkey press.
    pub fn on_widget(&mut self, input: WidgetInput, at: f64) {
        match (input, &self.state) {
            (WidgetInput::Click, State::Recording { hands_free: false }) => {
                self.state = State::Recording { hands_free: true };
            }
            (WidgetInput::Click, State::Recording { hands_free: true }) => {
                self.stop_and_process(at)
            }
            (WidgetInput::Click, s) if !s.is_busy() => self.start(at, true),
            (WidgetInput::HoldBegan, State::Recording { .. }) => {}
            (WidgetInput::HoldBegan, s) if !s.is_busy() => self.start(at, false),
            (WidgetInput::HoldEnded, State::Recording { hands_free: false }) => {
                self.stop_and_process(at);
            }
            _ => {}
        }
    }

    fn stop_and_process(&mut self, at: f64) {
        let samples = self.stop_recorder();
        // Too short to be speech: dropped silently (DictationSession.swift:49).
        if is_long_enough(samples.len()) {
            self.begin_processing(samples, at);
        } else {
            self.clear_preview();
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
        /// Non-zero but too quiet to be speech.
        quiet: Cell<bool>,
        /// How many samples the live recording holds, for the preview.
        captured: Cell<usize>,
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
            if self.0.quiet.get() {
                return vec![1e-4; n];
            }
            let mut v = vec![0.0; n / 2];
            v.extend(vec![0.1; n - n / 2]);
            v
        }
    }

    impl crate::live::LiveSource for FakeRecorder {
        fn captured(&self) -> usize {
            self.0.captured.get()
        }
        fn samples_from(&self, start: usize) -> Vec<f32> {
            vec![0.1; self.0.captured.get().saturating_sub(start)]
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
        for s in [
            State::LoadingModel,
            State::Transcribing,
            State::Formatting,
            State::Inserting,
        ] {
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
        assert_eq!(action_for(&State::Formatting, Event::Esc), Action::Cancel);
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
    fn quiet_audio_fails_with_nothing_recognized_and_makes_no_job() {
        let (mut c, shared) = setup_ready();
        shared.quiet.set(true);
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

    #[test]
    fn digital_silence_fails_with_the_microphone_hint_and_makes_no_job() {
        let (mut c, shared) = setup_ready();
        shared.silent.set(true);
        dictate(&mut c, 0.0);
        let v = c.view();
        assert_eq!(
            (v.phase, v.message.as_str()),
            (
                Phase::Failed,
                "Microphone: no sound, check Windows microphone privacy settings"
            )
        );
        assert_eq!(c.take_job(), None);
        c.poll(5.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    #[test]
    fn digital_silence_is_reported_before_the_model_is_ready() {
        let (mut c, shared) = setup();
        shared.silent.set(true);
        dictate(&mut c, 0.0);
        assert_eq!(c.view().phase, Phase::Failed);
        assert_eq!(c.take_job(), None);
    }

    #[test]
    fn the_silent_error_reads_like_the_access_denied_one() {
        assert_eq!(
            CaptureError::Silent.to_string(),
            "no sound, check Windows microphone privacy settings"
        );
    }

    // A chosen hotkey.

    fn alt_key(c: &mut Controller<FakeRecorder>, message: u32, at: f64) {
        c.on_key(
            RawKey {
                message,
                vk: 0xA5,
                flags: LLKHF_EXTENDED,
            },
            at,
        );
    }

    #[test]
    fn with_hotkey_right_alt_starts_on_right_alt_and_ignores_right_ctrl() {
        let (c, _shared) = setup_ready();
        let mut c = c.with_hotkey(Hotkey::RightAlt);
        press(&mut c, 0.0);
        release(&mut c, 0.5);
        assert_eq!(c.view().phase, Phase::Idle);
        alt_key(&mut c, WM_KEYDOWN, 1.0);
        assert_eq!(c.view().phase, Phase::Listening);
        alt_key(&mut c, WM_KEYUP, 2.0);
        assert_eq!(c.view().phase, Phase::Processing);
    }

    #[test]
    fn set_hotkey_switches_the_key_for_the_next_press() {
        let (mut c, _shared) = setup_ready();
        c.set_hotkey(Hotkey::RightAlt);
        press(&mut c, 0.0);
        assert_eq!(c.view().phase, Phase::Idle);
        alt_key(&mut c, WM_KEYDOWN, 1.0);
        assert_eq!(c.view().phase, Phase::Listening);
    }

    #[test]
    fn set_hotkey_drops_a_half_finished_gesture() {
        let (mut c, _shared) = setup_ready();
        // Right Ctrl goes down and Esc cancels the recording; the release is never seen.
        press(&mut c, 0.0);
        esc(&mut c, 0.5);
        assert_eq!(c.view().phase, Phase::Idle);
        c.set_hotkey(Hotkey::RightAlt);
        alt_key(&mut c, WM_KEYDOWN, 1.0);
        assert_eq!(c.view().phase, Phase::Listening);
    }

    #[test]
    fn set_hotkey_to_the_same_key_mid_press_does_not_stick() {
        let (mut c, _shared) = setup_ready();
        press(&mut c, 0.0);
        esc(&mut c, 0.5);
        c.set_hotkey(Hotkey::RightCtrl);
        press(&mut c, 1.0);
        assert_eq!(c.view().phase, Phase::Listening);
    }

    #[test]
    fn esc_still_cancels_after_the_hotkey_changed() {
        let (mut c, _shared) = setup_ready();
        c.set_hotkey(Hotkey::F13);
        c.on_key(
            RawKey {
                message: WM_KEYDOWN,
                vk: 0x7C,
                flags: 0,
            },
            0.0,
        );
        assert_eq!(c.view().phase, Phase::Listening);
        esc(&mut c, 0.5);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    fn transcribe(c: &mut Controller<FakeRecorder>, at: f64) -> u64 {
        dictate(c, at);
        c.take_job().expect("job").id
    }

    /// The polish worker's side: takes the format job and answers with `text` as the final text.
    fn format_with(c: &mut Controller<FakeRecorder>, id: u64, text: &str, at: f64) {
        let job = c.take_format().expect("format job");
        assert_eq!(job.id, id);
        c.on_formatted(id, text.into(), at);
    }

    #[test]
    fn ok_result_is_done_for_two_and_a_half_seconds() {
        let (mut c, _s) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        c.on_recognized(id, Ok("hello there".into()), 10.0);
        let v = c.view();
        // Accepted text goes through Formatting, then Inserting; Done starts when the insert
        // worker reports back.
        assert_eq!(
            (v.phase, v.stage),
            (Phase::Processing, Some(Stage::Formatting))
        );
        format_with(&mut c, id, "hello there", 10.2);
        let v = c.view();
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
        format_with(&mut c, new, "fresh", 6.2);
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
        format_with(&mut c, id, "hello", 2.2);
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
        format_with(c, id, text, 2.2);
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
            format_with(&mut c, id, "hello", 2.2);
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

    // Widget input (WidgetController.swift:51-53, DictationSession.swift:66-72, 129-136).

    fn failed_controller() -> (Controller<FakeRecorder>, Rc<Shared>) {
        let (mut c, shared) = setup_ready();
        *shared.start_error.borrow_mut() = Some(CaptureError::NoDevice);
        press(&mut c, 0.0);
        assert_eq!(c.view().phase, Phase::Failed);
        *shared.start_error.borrow_mut() = None;
        (c, shared)
    }

    fn done_controller() -> (Controller<FakeRecorder>, Rc<Shared>) {
        let (mut c, shared) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        c.on_recognized(id, Ok("hello there".into()), 2.0);
        format_with(&mut c, id, "hello there", 2.2);
        c.take_insert().expect("insert job");
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 2.5);
        assert_eq!(c.view().phase, Phase::Done);
        (c, shared)
    }

    fn assert_listening(c: &Controller<FakeRecorder>, hands_free: bool) {
        let v = c.view();
        assert_eq!((v.phase, v.hands_free), (Phase::Listening, hands_free));
    }

    #[test]
    fn a_widget_click_while_idle_starts_a_hands_free_recording() {
        let (mut c, shared) = setup_ready();
        c.on_widget(WidgetInput::Click, 0.0);
        assert_listening(&c, true);
        assert_eq!(shared.starts.get(), 1);
        assert!(
            c.take_capture().is_some(),
            "the context capture is requested"
        );
    }

    #[test]
    fn a_widget_click_after_done_starts_a_hands_free_recording() {
        let (mut c, shared) = done_controller();
        c.on_widget(WidgetInput::Click, 3.0);
        assert_listening(&c, true);
        assert_eq!(shared.starts.get(), 2);
    }

    #[test]
    fn a_widget_click_after_a_failure_starts_a_hands_free_recording() {
        let (mut c, shared) = failed_controller();
        c.on_widget(WidgetInput::Click, 1.0);
        assert_listening(&c, true);
        assert_eq!(shared.starts.get(), 2);
    }

    #[test]
    fn a_widget_click_while_push_to_talk_makes_it_hands_free_without_restarting() {
        let (mut c, shared) = setup_ready();
        press(&mut c, 0.0);
        assert_listening(&c, false);
        c.on_widget(WidgetInput::Click, 0.5);
        assert_listening(&c, true);
        assert_eq!((shared.starts.get(), shared.stops.get()), (1, 0));
    }

    #[test]
    fn a_widget_click_while_hands_free_stops_and_hands_over_the_audio() {
        let (mut c, shared) = setup_ready();
        c.on_widget(WidgetInput::Click, 0.0);
        c.on_widget(WidgetInput::Click, 1.0);
        assert_eq!(c.view().phase, Phase::Processing);
        assert_eq!(shared.stops.get(), 1);
        assert_eq!(c.take_job().map(|j| j.samples.len()), Some(16_000));
    }

    #[test]
    fn a_widget_hold_is_push_to_talk() {
        let (mut c, shared) = setup_ready();
        c.on_widget(WidgetInput::HoldBegan, 0.0);
        assert_listening(&c, false);
        c.on_widget(WidgetInput::HoldEnded, 1.0);
        assert_eq!(c.view().phase, Phase::Processing);
        assert_eq!((shared.starts.get(), shared.stops.get()), (1, 1));
        assert!(c.take_job().is_some());
    }

    #[test]
    fn a_widget_hold_can_start_after_done_and_after_a_failure() {
        let (mut c, _) = done_controller();
        c.on_widget(WidgetInput::HoldBegan, 3.0);
        assert_listening(&c, false);
        let (mut c, _) = failed_controller();
        c.on_widget(WidgetInput::HoldBegan, 1.0);
        assert_listening(&c, false);
    }

    #[test]
    fn a_hold_end_without_a_recording_does_nothing() {
        let (mut c, shared) = setup_ready();
        c.on_widget(WidgetInput::HoldEnded, 1.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(shared.stops.get(), 0);
    }

    #[test]
    fn a_hold_begin_during_a_recording_does_not_restart_it() {
        let (mut c, shared) = setup_ready();
        c.on_widget(WidgetInput::Click, 0.0);
        c.on_widget(WidgetInput::HoldBegan, 0.5);
        assert_listening(&c, true);
        assert_eq!(shared.starts.get(), 1);
    }

    #[test]
    fn widget_input_is_ignored_while_the_model_loads() {
        let (mut c, shared) = setup();
        dictate(&mut c, 0.0);
        assert_eq!(c.view().stage, Some(Stage::Loading));
        for input in [
            WidgetInput::Click,
            WidgetInput::HoldBegan,
            WidgetInput::HoldEnded,
        ] {
            c.on_widget(input, 5.0);
            assert_eq!(c.view().stage, Some(Stage::Loading), "{input:?}");
        }
        assert_eq!(shared.starts.get(), 1);
    }

    #[test]
    fn widget_input_is_ignored_while_transcribing_and_inserting() {
        let (mut c, shared) = setup_ready();
        let id = transcribe(&mut c, 0.0);
        assert_eq!(c.view().stage, Some(Stage::Transcribing));
        for input in [
            WidgetInput::Click,
            WidgetInput::HoldBegan,
            WidgetInput::HoldEnded,
        ] {
            c.on_widget(input, 5.0);
            assert_eq!(c.view().stage, Some(Stage::Transcribing), "{input:?}");
        }
        c.on_recognized(id, Ok("hello there".into()), 6.0);
        for input in [
            WidgetInput::Click,
            WidgetInput::HoldBegan,
            WidgetInput::HoldEnded,
        ] {
            c.on_widget(input, 6.5);
            assert_eq!(c.view().stage, Some(Stage::Formatting), "{input:?}");
        }
        format_with(&mut c, id, "hello there", 6.8);
        for input in [
            WidgetInput::Click,
            WidgetInput::HoldBegan,
            WidgetInput::HoldEnded,
        ] {
            c.on_widget(input, 7.0);
            assert_eq!(c.view().stage, Some(Stage::Inserting), "{input:?}");
        }
        assert_eq!(shared.starts.get(), 1);
    }

    #[test]
    fn the_view_target_is_the_app_name_once_the_context_arrives() {
        let (mut c, _) = setup_ready();
        c.on_widget(WidgetInput::Click, 0.0);
        let id = c.take_capture().expect("capture id");
        assert_eq!(c.view().target, None);
        c.on_context(
            id,
            TargetContext {
                app_name: Some("Notepad".into()),
                ..TargetContext::default()
            },
        );
        assert_eq!(c.view().target.as_deref(), Some("Notepad"));
    }

    #[test]
    fn a_context_without_an_app_name_leaves_the_target_empty() {
        let (mut c, _) = setup_ready();
        c.on_widget(WidgetInput::Click, 0.0);
        let id = c.take_capture().expect("capture id");
        c.on_context(id, TargetContext::default());
        assert_eq!(c.view().target, None);
    }

    #[test]
    fn a_stale_context_does_not_set_the_target() {
        let (mut c, _) = setup_ready();
        c.on_widget(WidgetInput::Click, 0.0);
        let id = c.take_capture().expect("capture id");
        c.on_context(
            id + 100,
            TargetContext {
                app_name: Some("Old".into()),
                ..TargetContext::default()
            },
        );
        assert_eq!(c.view().target, None);
    }

    #[test]
    fn the_target_is_cleared_when_the_next_dictation_starts() {
        let (mut c, _) = setup_ready();
        press(&mut c, 0.0);
        let id = c.take_capture().expect("capture id");
        c.on_context(
            id,
            TargetContext {
                app_name: Some("Notepad".into()),
                ..TargetContext::default()
            },
        );
        release(&mut c, 0.1);
        assert_eq!(c.view().phase, Phase::Idle);
        c.on_widget(WidgetInput::Click, 5.0);
        assert_eq!(c.view().target, None);
    }

    // Formatting (Windows task 9): between the accepted transcript and the insert.

    /// A dictation whose transcript was accepted and is being formatted.
    fn formatting(c: &mut Controller<FakeRecorder>, text: &str) -> u64 {
        let id = start_dictation(c, 0.0);
        c.take_job().expect("job");
        c.on_recognized(id, Ok(text.into()), 2.0);
        id
    }

    #[test]
    fn formatting_is_busy_and_shows_the_formatting_stage() {
        let (mut c, shared) = setup_ready();
        formatting(&mut c, "hello");
        let v = c.view();
        assert_eq!(
            (v.phase, v.stage),
            (Phase::Processing, Some(Stage::Formatting))
        );
        press(&mut c, 3.0);
        assert_eq!(
            shared.starts.get(),
            1,
            "a press while formatting is ignored"
        );
        assert_eq!(c.view().stage, Some(Stage::Formatting));
    }

    #[test]
    fn no_start_can_replace_a_run_that_is_still_formatting_or_inserting() {
        // `start` drops `run` without finishing it. That is safe only because every way to start
        // is refused while busy: a hotkey press and both widget inputs.
        type Reach = fn(&mut Controller<FakeRecorder>) -> u64;
        let stages: [(&str, Reach); 2] = [
            ("formatting", |c| {
                formatting_with(c, Some(app_context(Some(false))))
            }),
            ("inserting", |c| {
                inserting_with(c, Some(app_context(Some(false))), meta())
            }),
        ];
        for (what, reach) in stages {
            let (mut c, shared) = setup_ready();
            let id = reach(&mut c);
            press(&mut c, 3.0);
            c.on_widget(WidgetInput::Click, 3.1);
            c.on_widget(WidgetInput::HoldBegan, 3.2);
            assert_eq!(shared.starts.get(), 1, "{what}: a start got through");
            esc(&mut c, 3.3);
            let f = c
                .take_finished()
                .unwrap_or_else(|| panic!("{what}: run lost"));
            assert_eq!(f.id, id, "{what}");
        }
    }

    #[test]
    fn an_accepted_transcript_gives_a_format_job_once() {
        let (mut c, _s) = setup_ready();
        let id = formatting(&mut c, "hello there");
        assert_eq!(
            c.take_format(),
            Some(FormatJob {
                id,
                transcript: "hello there".into(),
                context: None,
            })
        );
        assert_eq!(c.take_format(), None);
        assert_eq!(
            c.take_insert(),
            None,
            "nothing is inserted before it is formatted"
        );
    }

    #[test]
    fn the_format_job_carries_the_captured_context() {
        let (mut c, _s) = setup_ready();
        press(&mut c, 0.0);
        let id = c.take_capture().unwrap();
        let ctx = TargetContext {
            app_id: Some("notepad.exe".into()),
            app_name: Some("Notepad".into()),
            window_title: Some("notes.txt".into()),
            password: Some(false),
            ..Default::default()
        };
        c.on_context(id, ctx.clone());
        release(&mut c, 1.0);
        c.take_job().unwrap();
        c.on_recognized(id, Ok("hello".into()), 2.0);
        assert_eq!(c.take_format().unwrap().context, Some(ctx));
    }

    #[test]
    fn a_blank_or_junk_transcript_makes_no_format_job() {
        for text in ["   ", "", "Thank you."] {
            let (mut c, _s) = setup_ready();
            let id = transcribe(&mut c, 0.0);
            c.on_recognized(id, Ok(text.into()), 2.0);
            assert_eq!(c.take_format(), None, "{text:?}");
            assert_eq!(c.formatting_id(), None, "{text:?}");
        }
    }

    #[test]
    fn formatting_id_is_the_dictation_while_formatting_only() {
        let (mut c, _s) = setup_ready();
        assert_eq!(c.formatting_id(), None);
        let id = formatting(&mut c, "hello");
        assert_eq!(c.formatting_id(), Some(id));
        format_with(&mut c, id, "Hello", 3.0);
        assert_eq!(c.formatting_id(), None);
    }

    #[test]
    fn formatting_id_is_cleared_by_esc() {
        let (mut c, _s) = setup_ready();
        formatting(&mut c, "hello");
        esc(&mut c, 2.5);
        assert_eq!(c.formatting_id(), None);
    }

    #[test]
    fn on_formatted_moves_to_inserting_with_the_final_text() {
        let (mut c, _s) = setup_ready();
        let id = formatting(&mut c, "hello there");
        c.take_format().unwrap();
        c.on_formatted(id, "Hello, there.".into(), 3.0);
        let v = c.view();
        assert_eq!(
            (v.phase, v.stage),
            (Phase::Processing, Some(Stage::Inserting))
        );
        let job = c.take_insert().unwrap();
        assert_eq!((job.id, job.text.as_str()), (id, "Hello, there."));
        assert_eq!(c.take_insert(), None);
    }

    #[test]
    fn the_insert_job_after_formatting_carries_the_captured_password_flag() {
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
            c.take_format().unwrap();
            c.on_formatted(id, "Hello".into(), 2.5);
            let job = c.take_insert().unwrap();
            assert_eq!((job.id, job.captured_password), (id, flag), "{flag:?}");
        }
    }

    #[test]
    fn the_done_preview_is_the_formatted_text_not_the_transcript() {
        let (mut c, _s) = setup_ready();
        let id = formatting(&mut c, "hello there");
        format_with(&mut c, id, "Hello there, friend.", 3.0);
        c.take_insert().unwrap();
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.5);
        assert_eq!(c.view().message, "Hello there, friend.");
    }

    #[test]
    fn esc_while_formatting_goes_idle_at_once() {
        let (mut c, _s) = setup_ready();
        formatting(&mut c, "hello");
        esc(&mut c, 2.5);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_format(), None, "an untaken format job is dropped");
    }

    // The riskiest control: a late agent reply (up to 25 s) must not paste cancelled text into
    // whatever app has focus. The guard is the id and state check in `on_formatted`.

    #[test]
    fn esc_while_formatting_drops_the_late_result() {
        let (mut c, _s) = setup_ready();
        let id = formatting(&mut c, "hello there");
        c.take_format().unwrap();
        esc(&mut c, 2.5);
        c.on_formatted(id, "Late text from the agent".into(), 20.0);
        assert_eq!(c.view().phase, Phase::Idle);
        assert_eq!(c.take_insert(), None, "no insert job for cancelled text");
    }

    #[test]
    fn an_old_formatted_result_is_ignored_while_a_newer_dictation_formats() {
        let (mut c, _s) = setup_ready();
        let old = formatting(&mut c, "first dictation");
        c.take_format().unwrap();
        esc(&mut c, 2.5);
        press(&mut c, 3.0);
        let new = c.take_capture().expect("capture id");
        release(&mut c, 4.0);
        assert_ne!(old, new);
        let job = c.take_job().unwrap();
        c.on_recognized(job.id, Ok("second dictation".into()), 5.0);
        c.take_format().unwrap();
        // The old agent reply arrives while the new dictation is formatting.
        c.on_formatted(old, "Stale text".into(), 6.0);
        assert_eq!(c.view().stage, Some(Stage::Formatting));
        assert_eq!(c.take_insert(), None);
        assert_eq!(c.formatting_id(), Some(new));
        c.on_formatted(new, "Fresh text".into(), 7.0);
        assert_eq!(c.take_insert().map(|j| j.text), Some("Fresh text".into()));
    }

    #[test]
    fn a_formatted_result_outside_the_formatting_state_is_ignored() {
        // Idle, never started.
        let (mut c, _s) = setup_ready();
        c.on_formatted(1, "text".into(), 1.0);
        assert_eq!(c.take_insert(), None);
        // Already inserting: a duplicate answer does not queue a second insert.
        let id = formatting(&mut c, "hello");
        format_with(&mut c, id, "Hello", 3.0);
        c.take_insert().unwrap();
        c.on_formatted(id, "Duplicate".into(), 3.1);
        assert_eq!(c.take_insert(), None);
        assert_eq!(c.view().stage, Some(Stage::Inserting));
    }

    #[test]
    fn a_formatted_result_for_another_id_is_ignored() {
        let (mut c, _s) = setup_ready();
        let id = formatting(&mut c, "hello");
        c.take_format().unwrap();
        c.on_formatted(id + 1, "Other".into(), 3.0);
        assert_eq!(c.view().stage, Some(Stage::Formatting));
        assert_eq!(c.take_insert(), None);
    }

    #[test]
    fn esc_while_inserting_after_formatting_still_drops_the_outcome() {
        let (mut c, _s) = setup_ready();
        let id = formatting(&mut c, "hello");
        format_with(&mut c, id, "Hello", 3.0);
        esc(&mut c, 3.5);
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 4.0);
        assert_eq!(c.view().phase, Phase::Idle);
    }

    // History (Windows task 10): what a finished dictation hands to the history worker.

    /// What `FakeRecorder` returns for one second.
    fn one_second() -> Vec<f32> {
        let mut v = vec![0.0; 8_000];
        v.extend(vec![0.1; 8_000]);
        v
    }

    fn app_context(password: Option<bool>) -> TargetContext {
        TargetContext {
            app_id: Some("code.exe".into()),
            app_name: Some("Visual Studio Code".into()),
            window_title: Some("main.rs - shepit".into()),
            password,
            ..Default::default()
        }
    }

    fn meta() -> FormatMeta {
        FormatMeta {
            profile: "generic".into(),
            mode: "clean".into(),
            source: Source::Claude,
            language: Some("en".into()),
            new_terms: vec!["Vercel".into()],
            llm_ms: 420,
        }
    }

    /// Records one second with the context (if any) already captured. The recognizer job is not taken.
    fn begin(c: &mut Controller<FakeRecorder>, context: Option<TargetContext>) -> u64 {
        press(c, 0.0);
        let id = c.take_capture().expect("capture id");
        if let Some(context) = context {
            c.on_context(id, context);
        }
        release(c, 1.0);
        id
    }

    /// Recognized in 350 ms; the format job is waiting.
    fn formatting_with(c: &mut Controller<FakeRecorder>, context: Option<TargetContext>) -> u64 {
        let id = begin(c, context);
        c.take_job().expect("job");
        c.on_stt_ms(id, 350);
        c.on_recognized(id, Ok("hello there".into()), 2.0);
        c.take_format().expect("format job");
        id
    }

    /// Formatted as "Hello there." and waiting for the insert.
    fn inserting_with(
        c: &mut Controller<FakeRecorder>,
        context: Option<TargetContext>,
        meta: FormatMeta,
    ) -> u64 {
        let id = formatting_with(c, context);
        c.on_formatted_with(id, "Hello there.".into(), meta, 2.2);
        c.take_insert().expect("insert job");
        id
    }

    fn assert_nothing_kept(c: &mut Controller<FakeRecorder>, what: &str) {
        if let Some(f) = c.take_finished() {
            assert!(f.audio.is_none(), "{what}: audio kept");
            assert!(f.record.is_none(), "{what}: row kept");
        }
    }

    #[test]
    fn finished_after_done_has_record_and_audio() {
        let (mut c, _s) = setup_ready();
        let id = inserting_with(&mut c, Some(app_context(Some(false))), meta());
        assert_eq!(c.take_finished(), None, "not before the insert answers");
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
        let f = c.take_finished().expect("finished");
        assert_eq!(f.id, id);
        assert_eq!(f.audio, Some(one_second()));
        assert_eq!(
            f.record,
            Some(RecordDraft {
                app_id: Some("code.exe".into()),
                app_name: Some("Visual Studio Code".into()),
                window_title: Some("main.rs - shepit".into()),
                profile: "generic".into(),
                mode: "clean".into(),
                engine: "parakeet-v3".into(),
                raw_text: "hello there".into(),
                formatted_text: "Hello there.".into(),
                language: Some("en".into()),
                audio_seconds: 1.0,
                stt_ms: 350,
                llm_ms: 420,
                formatter_source: "claude".into(),
                insert_method: "paste".into(),
                status: "inserted".into(),
            })
        );
        assert_eq!(c.take_finished(), None, "handed over once");
    }

    #[test]
    fn a_clipboard_only_insert_is_a_clipboard_row() {
        let (mut c, _s) = setup_ready();
        let id = inserting_with(&mut c, Some(app_context(Some(false))), meta());
        c.on_inserted(id, Ok(InsertOutcome::ClipboardOnly), 3.0);
        let r = c.take_finished().and_then(|f| f.record).expect("row");
        assert_eq!(
            (r.insert_method.as_str(), r.status.as_str()),
            ("clipboardOnly", "clipboard")
        );
    }

    #[test]
    fn a_dictation_without_a_context_has_no_app_and_still_a_row() {
        let (mut c, _s) = setup_ready();
        let id = inserting_with(&mut c, None, meta());
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
        let f = c.take_finished().expect("finished");
        assert!(f.audio.is_some());
        let r = f.record.expect("row");
        assert_eq!((r.app_id, r.app_name, r.window_title), (None, None, None));
    }

    #[test]
    fn the_row_language_is_the_format_result_or_none() {
        let (mut c, _s) = setup_ready();
        let mut m = meta();
        m.language = None;
        let id = inserting_with(&mut c, None, m);
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
        assert_eq!(
            c.take_finished().and_then(|f| f.record).unwrap().language,
            None
        );
    }

    #[test]
    fn the_row_source_is_the_result_source_id() {
        for (source, id) in [
            (Source::Claude, "claude"),
            (Source::Codex, "codex"),
            (Source::Passthrough, "passthrough"),
            (Source::FastPath, "fastPath"),
            (Source::Fallback, "fallback"),
        ] {
            let (mut c, _s) = setup_ready();
            let mut m = meta();
            m.source = source;
            let dictation = inserting_with(&mut c, None, m);
            c.on_inserted(dictation, Ok(InsertOutcome::Pasted), 3.0);
            let r = c.take_finished().and_then(|f| f.record).unwrap();
            assert_eq!(r.formatter_source, id);
        }
    }

    #[test]
    fn the_stt_time_of_another_dictation_is_ignored() {
        let (mut c, _s) = setup_ready();
        let id = begin(&mut c, None);
        c.take_job().expect("job");
        c.on_stt_ms(id + 100, 9_999);
        c.on_recognized(id, Ok("hello there".into()), 2.0);
        c.take_format().expect("format job");
        c.on_formatted_with(id, "Hello there.".into(), meta(), 2.2);
        c.take_insert().expect("insert job");
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
        assert_eq!(c.take_finished().and_then(|f| f.record).unwrap().stt_ms, 0);
    }

    #[test]
    fn a_stale_insert_answer_finishes_nothing() {
        let (mut c, _s) = setup_ready();
        let id = inserting_with(&mut c, None, meta());
        c.on_inserted(id + 1, Ok(InsertOutcome::Pasted), 3.0);
        assert_eq!(c.take_finished(), None);
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.1);
        assert_eq!(c.take_finished().map(|f| f.id), Some(id));
    }

    #[test]
    fn speech_gate_failure_keeps_audio_only() {
        for (what, quiet, silent) in [("quiet room", true, false), ("muted mic", false, true)] {
            let (mut c, shared) = setup_ready();
            shared.quiet.set(quiet);
            shared.silent.set(silent);
            let id = begin(&mut c, Some(app_context(Some(false))));
            assert!(matches!(c.view().phase, Phase::Failed), "{what}");
            let f = c.take_finished().expect(what);
            assert_eq!(f.id, id, "{what}");
            assert_eq!(f.audio.map(|a| a.len()), Some(16_000), "{what}");
            assert_eq!(f.record, None, "{what}");
            assert_eq!(c.take_finished(), None, "{what}: once");
        }
    }

    #[test]
    fn recognizer_errors_and_junk_keep_audio_only() {
        let results: [Result<String, String>; 3] = [
            Err("Transcription failed: boom".into()),
            Ok("   ".into()),
            Ok("Thank you.".into()),
        ];
        for result in results {
            let (mut c, _s) = setup_ready();
            let id = begin(&mut c, Some(app_context(Some(false))));
            c.take_job().expect("job");
            c.on_recognized(id, result.clone(), 2.0);
            let f = c.take_finished().unwrap_or_else(|| panic!("{result:?}"));
            assert_eq!(
                (f.id, f.audio.is_some(), f.record),
                (id, true, None),
                "{result:?}"
            );
        }
    }

    #[test]
    fn an_insert_error_keeps_audio_only() {
        let (mut c, _s) = setup_ready();
        let id = inserting_with(&mut c, Some(app_context(Some(false))), meta());
        c.on_inserted(id, Err(crate::insert::CLIPBOARD_BUSY.into()), 3.0);
        let f = c.take_finished().expect("finished");
        assert_eq!((f.id, f.audio.is_some(), f.record), (id, true, None));
    }

    #[test]
    fn esc_while_busy_keeps_audio_only() {
        type Reach = fn(&mut Controller<FakeRecorder>) -> u64;
        let stages: [(&str, Reach); 3] = [
            ("transcribing", |c| {
                let id = begin(c, Some(app_context(Some(false))));
                c.take_job().expect("job");
                id
            }),
            ("formatting", |c| {
                formatting_with(c, Some(app_context(Some(false))))
            }),
            ("inserting", |c| {
                inserting_with(c, Some(app_context(Some(false))), meta())
            }),
        ];
        for (what, reach) in stages {
            let (mut c, _s) = setup_ready();
            let id = reach(&mut c);
            esc(&mut c, 2.5);
            let f = c.take_finished().unwrap_or_else(|| panic!("{what}"));
            assert_eq!(
                (f.id, f.audio.is_some(), f.record),
                (id, true, None),
                "{what}"
            );
        }
    }

    #[test]
    fn esc_while_recording_and_too_short_finish_nothing() {
        let (mut c, shared) = setup_ready();
        press(&mut c, 0.0);
        esc(&mut c, 0.5);
        // The key comes up before the next press, as it does on a keyboard; the gesture ignores a
        // second down without an up as auto-repeat.
        release(&mut c, 0.6);
        assert_eq!(c.take_finished(), None, "esc while recording");
        shared.stop_len.set(6399);
        begin(&mut c, None);
        assert_eq!(c.take_finished(), None, "under 0.4 s");
        esc(&mut c, 3.0);
        assert_eq!(c.take_finished(), None, "esc when idle");
    }

    #[test]
    fn nothing_finishes_while_the_dictation_is_still_running() {
        let (mut c, _s) = setup_ready();
        let id = begin(&mut c, None);
        assert_eq!(c.take_finished(), None, "transcribing");
        c.take_job().expect("job");
        c.on_recognized(id, Ok("hello there".into()), 2.0);
        assert_eq!(c.take_finished(), None, "formatting");
        c.on_formatted_with(id, "Hello.".into(), meta(), 2.2);
        assert_eq!(c.take_finished(), None, "inserting");
    }

    #[test]
    fn password_dictation_keeps_no_audio() {
        // Captured Some(true), every way the dictation can end.
        type End = fn(&mut Controller<FakeRecorder>);
        let ends: [(&str, End); 6] = [
            ("pasted", |c| {
                let id = inserting_with(c, Some(app_context(Some(true))), meta());
                c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
            }),
            ("clipboard only", |c| {
                let id = inserting_with(c, Some(app_context(Some(true))), meta());
                c.on_inserted(id, Ok(InsertOutcome::ClipboardOnly), 3.0);
            }),
            ("insert error", |c| {
                let id = inserting_with(c, Some(app_context(Some(true))), meta());
                c.on_inserted(id, Err(crate::insert::CLIPBOARD_BUSY.into()), 3.0);
            }),
            ("recognizer error", |c| {
                let id = begin(c, Some(app_context(Some(true))));
                c.take_job().expect("job");
                c.on_recognized(id, Err("Transcription failed: boom".into()), 2.0);
            }),
            ("esc while formatting", |c| {
                formatting_with(c, Some(app_context(Some(true))));
                esc(c, 2.5);
            }),
            ("esc while transcribing", |c| {
                begin(c, Some(app_context(Some(true))));
                c.take_job().expect("job");
                esc(c, 1.5);
            }),
        ];
        // The control: the same dictation in a clear field is kept, so the loop below cannot pass
        // just because nothing is ever kept.
        let (mut c, _s) = setup_ready();
        let id = inserting_with(&mut c, Some(app_context(Some(false))), meta());
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
        assert!(c.take_finished().is_some_and(|f| f.audio.is_some()));
        for (what, end) in ends {
            let (mut c, _s) = setup_ready();
            end(&mut c);
            assert_nothing_kept(&mut c, what);
        }
        let (mut c, shared) = setup_ready();
        shared.quiet.set(true);
        begin(&mut c, Some(app_context(Some(true))));
        assert_nothing_kept(&mut c, "speech gate");
    }

    #[test]
    fn an_early_end_keeps_the_audio_only_when_the_flag_says_clear() {
        // Digital silence, the speech gate, a recognizer error, Esc and an insert error all end
        // without a successful insert. A flag that was never read or could not be read (None)
        // is not proof the field was clear, so only Some(false) keeps the audio.
        type End = fn(&mut Controller<FakeRecorder>, Option<bool>);
        let ends: [(&str, End); 6] = [
            ("digital silence", |c, flag| {
                begin(c, Some(app_context(flag)));
            }),
            ("speech gate", |c, flag| {
                begin(c, Some(app_context(flag)));
            }),
            ("recognizer error", |c, flag| {
                let id = begin(c, Some(app_context(flag)));
                c.take_job().expect("job");
                c.on_recognized(id, Err("Transcription failed: boom".into()), 2.0);
            }),
            ("esc while formatting", |c, flag| {
                formatting_with(c, Some(app_context(flag)));
                esc(c, 2.5);
            }),
            ("insert error", |c, flag| {
                let id = inserting_with(c, Some(app_context(flag)), meta());
                c.on_inserted(id, Err(crate::insert::CLIPBOARD_BUSY.into()), 3.0);
            }),
            ("context never arrived", |c, _| {
                let id = formatting_with(c, None);
                c.on_formatted_with(id, "x".into(), meta(), 2.2);
                c.take_insert().expect("insert job");
                esc(c, 2.5);
            }),
        ];
        for (what, end) in ends {
            for flag in [None, Some(true), Some(false)] {
                let (mut c, shared) = setup_ready();
                match what {
                    "digital silence" => shared.silent.set(true),
                    "speech gate" => shared.quiet.set(true),
                    _ => {}
                }
                end(&mut c, flag);
                if what == "context never arrived" || flag != Some(false) {
                    assert_nothing_kept(&mut c, &format!("{what} with {flag:?}"));
                    assert_eq!(c.take_finished(), None);
                } else {
                    let f = c.take_finished().unwrap_or_else(|| panic!("{what}"));
                    assert!(f.audio.is_some() && f.record.is_none(), "{what}");
                }
            }
        }
    }

    #[test]
    fn an_unreadable_or_clear_password_flag_keeps_the_audio() {
        for flag in [Some(false), None] {
            let (mut c, _s) = setup_ready();
            let id = inserting_with(&mut c, Some(app_context(flag)), meta());
            c.on_inserted(id, Ok(InsertOutcome::Pasted), 3.0);
            let f = c.take_finished().expect("finished");
            assert!(f.audio.is_some() && f.record.is_some(), "{flag:?}");
        }
    }

    #[test]
    fn a_refused_insert_keeps_no_audio_and_no_row() {
        // The password field was focused after recording started: the captured flag said no, the
        // check before the paste said yes.
        for flag in [Some(false), None] {
            let (mut c, _s) = setup_ready();
            let id = inserting_with(&mut c, Some(app_context(flag)), meta());
            c.on_inserted(id, Err(crate::insert::PASSWORD_REFUSED.into()), 3.0);
            assert_nothing_kept(&mut c, &format!("refused after {flag:?}"));
        }
    }

    #[test]
    fn new_terms_come_out_once_for_the_live_dictation() {
        let (mut c, _s) = setup_ready();
        let id = formatting_with(&mut c, None);
        assert!(c.take_new_terms().is_empty(), "before the result");
        c.on_formatted_with(id, "Hello there.".into(), meta(), 2.2);
        assert_eq!(c.take_new_terms(), ["Vercel"]);
        assert!(c.take_new_terms().is_empty(), "handed over once");
    }

    #[test]
    fn esc_or_a_stale_id_gives_no_new_terms() {
        let (mut c, _s) = setup_ready();
        let id = formatting_with(&mut c, None);
        c.on_formatted_with(id + 1, "Late.".into(), meta(), 2.2);
        assert!(c.take_new_terms().is_empty(), "stale id");
        c.on_formatted_with(id, "On time.".into(), meta(), 2.3);
        assert_eq!(c.take_new_terms(), ["Vercel"], "the live id still works");
        let (mut c, _s) = setup_ready();
        let id = formatting_with(&mut c, None);
        esc(&mut c, 2.4);
        c.on_formatted_with(id, "Cancelled.".into(), meta(), 2.5);
        assert!(c.take_new_terms().is_empty(), "after esc");

        let (mut c, _s) = setup_ready();
        let id = formatting_with(&mut c, None);
        c.on_formatted_with(id, "Hello.".into(), meta(), 2.2);
        esc(&mut c, 2.3);
        assert!(c.take_new_terms().is_empty(), "esc before they were taken");
    }

    // Live preview wiring (docs/_working/plan-windows-live.md, chunk 2).

    fn live_ready() -> (Controller<FakeRecorder>, Rc<Shared>) {
        let (mut c, shared) = setup_ready();
        c.set_live_preview(true);
        (c, shared)
    }

    /// Presses the key and returns the dictation id.
    fn start_live(c: &mut Controller<FakeRecorder>) -> u64 {
        press(c, 0.0);
        c.take_capture().expect("capture id")
    }

    /// One second is in the buffer and the first pass is handed out.
    fn first_pass(c: &mut Controller<FakeRecorder>, shared: &Shared) -> PreviewJob {
        shared.captured.set(16_000);
        c.poll(1.0);
        c.take_preview().expect("preview job at 1 s")
    }

    fn show(c: &mut Controller<FakeRecorder>, job: &PreviewJob, text: &str) {
        c.on_previewed(job.id, job.seq, Ok(text.into()), 0.1, 1.2);
    }

    #[test]
    fn no_preview_before_one_second_of_audio() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        shared.captured.set(15_999);
        c.poll(1.0);
        assert!(c.take_preview().is_none());
    }

    #[test]
    fn no_preview_while_the_model_is_not_ready() {
        for status in [
            ModelStatus::Loading,
            ModelStatus::Downloading(0.5),
            ModelStatus::Failed,
        ] {
            let (mut c, shared) = setup();
            c.on_model(status);
            c.set_live_preview(true);
            start_live(&mut c);
            shared.captured.set(32_000);
            c.poll(2.0);
            assert!(c.take_preview().is_none(), "{status:?}");
        }
    }

    #[test]
    fn the_preview_starts_once_the_model_is_ready() {
        let (mut c, shared) = setup();
        c.set_live_preview(true);
        start_live(&mut c);
        shared.captured.set(32_000);
        c.poll(2.0);
        assert!(c.take_preview().is_none());
        c.on_model(ModelStatus::Ready);
        c.poll(2.1);
        assert!(c.take_preview().is_some());
    }

    #[test]
    fn no_preview_when_the_setting_is_off_at_start() {
        let (mut c, shared) = live_ready();
        c.set_live_preview(false);
        start_live(&mut c);
        shared.captured.set(32_000);
        c.poll(2.0);
        assert!(c.take_preview().is_none());
        assert_eq!(c.preview_wanted(), None);
    }

    #[test]
    fn the_setting_is_read_at_start_only() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        c.set_live_preview(false);
        shared.captured.set(16_000);
        c.poll(1.0);
        assert!(c.take_preview().is_some(), "on at start stays on");
        esc(&mut c, 1.5);

        let (mut c, shared) = live_ready();
        c.set_live_preview(false);
        start_live(&mut c);
        c.set_live_preview(true);
        shared.captured.set(16_000);
        c.poll(1.0);
        assert!(c.take_preview().is_none(), "off at start stays off");
    }

    #[test]
    fn the_first_job_has_the_dictation_id_a_seq_and_one_second() {
        let (mut c, shared) = live_ready();
        let id = start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        assert_eq!(job.id, id);
        assert_eq!(job.samples.len(), 16_000);
        assert_eq!(c.preview_wanted(), Some(id));
    }

    #[test]
    fn a_preview_job_is_handed_out_once() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        first_pass(&mut c, &shared);
        assert!(c.take_preview().is_none());
        c.poll(1.1);
        assert!(c.take_preview().is_none(), "one pass in flight");
    }

    #[test]
    fn a_preview_result_shows_in_the_view() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        assert!(c.view().live.is_empty());
        show(&mut c, &job, "so the widget should");
        let v = c.view();
        assert_eq!(v.phase, Phase::Listening);
        assert_eq!(v.live.stable, "so the");
        assert_eq!(v.live.unstable, "widget should");
    }

    #[test]
    fn a_result_for_the_wrong_id_or_seq_is_ignored() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        c.on_previewed(job.id + 99, job.seq, Ok("wrong id here".into()), 0.1, 1.2);
        c.on_previewed(job.id, job.seq + 99, Ok("wrong seq here".into()), 0.1, 1.2);
        assert!(c.view().live.is_empty());
        show(&mut c, &job, "right words here");
        assert_eq!(c.view().live.full(), "right words here");
    }

    #[test]
    fn a_result_from_an_earlier_dictation_is_ignored() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let old = first_pass(&mut c, &shared);
        esc(&mut c, 1.1);
        release(&mut c, 1.2);
        press(&mut c, 5.0);
        c.take_capture().expect("capture id");
        shared.captured.set(0);
        c.on_previewed(old.id, old.seq, Ok("stale words here".into()), 0.1, 1.5);
        assert!(c.view().live.is_empty());
    }

    #[test]
    fn release_stops_the_previews_and_keeps_the_text() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "so the widget should");
        shared.captured.set(48_000);
        release(&mut c, 3.0);
        assert_eq!(c.preview_wanted(), None);
        c.poll(3.1);
        assert!(c.take_preview().is_none());
        let v = c.view();
        assert_eq!(v.phase, Phase::Processing);
        assert_eq!(v.live.full(), "so the widget should");
    }

    #[test]
    fn release_drops_a_job_that_was_not_taken() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        shared.captured.set(16_000);
        c.poll(1.0);
        release(&mut c, 1.5);
        assert!(c.take_preview().is_none());
        assert_eq!(c.preview_wanted(), None);
    }

    #[test]
    fn a_late_preview_result_after_release_is_ignored() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "so the widget should");
        c.poll(1.8);
        let second = c.take_preview().expect("second pass");
        release(&mut c, 2.0);
        c.on_previewed(
            second.id,
            second.seq,
            Ok("different late words".into()),
            0.1,
            2.1,
        );
        assert_eq!(c.view().live.full(), "so the widget should");
    }

    #[test]
    fn preview_text_never_reaches_the_insert() {
        let (mut c, shared) = live_ready();
        let id = start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words");
        release(&mut c, 1.5);
        let rec = c.take_job().expect("final job");
        assert_eq!(rec.id, id);
        c.on_recognized(id, Ok("final words".into()), 2.0);
        let fmt = c.take_format().expect("format job");
        assert_eq!(fmt.transcript, "final words");
        c.on_formatted(id, "final words".into(), 2.2);
        let ins = c.take_insert().expect("insert job");
        assert_eq!(ins.text, "final words");
        assert_eq!(
            c.view().live.full(),
            "preview words",
            "still on screen while inserting"
        );
    }

    #[test]
    fn done_clears_the_text() {
        let (mut c, shared) = live_ready();
        let id = start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        release(&mut c, 1.5);
        c.take_job().expect("job");
        c.on_recognized(id, Ok("final words".into()), 2.0);
        c.on_formatted(id, "final words".into(), 2.2);
        c.take_insert().expect("insert");
        c.on_inserted(id, Ok(InsertOutcome::Pasted), 2.4);
        let v = c.view();
        assert_eq!(v.phase, Phase::Done);
        assert!(v.live.is_empty());
    }

    #[test]
    fn a_failed_recognition_clears_the_text() {
        let (mut c, shared) = live_ready();
        let id = start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        release(&mut c, 1.5);
        c.take_job().expect("job");
        c.on_recognized(id, Err("boom".into()), 2.0);
        let v = c.view();
        assert_eq!(v.phase, Phase::Failed);
        assert!(v.live.is_empty());
    }

    #[test]
    fn a_failed_insert_clears_the_text() {
        let (mut c, shared) = live_ready();
        let id = start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        release(&mut c, 1.5);
        c.take_job().expect("job");
        c.on_recognized(id, Ok("final words".into()), 2.0);
        c.on_formatted(id, "final words".into(), 2.2);
        c.take_insert().expect("insert");
        c.on_inserted(id, Err("no target".into()), 2.4);
        let v = c.view();
        assert_eq!(v.phase, Phase::Failed);
        assert!(v.live.is_empty());
    }

    #[test]
    fn a_nothing_recognized_failure_clears_the_text() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        shared.quiet.set(true);
        release(&mut c, 1.5);
        let v = c.view();
        assert_eq!(v.phase, Phase::Failed);
        assert!(v.live.is_empty());
    }

    #[test]
    fn esc_while_recording_clears_the_text_and_stops_the_previews() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        esc(&mut c, 1.5);
        assert!(c.view().live.is_empty());
        assert_eq!(c.preview_wanted(), None);
        c.poll(2.0);
        assert!(c.take_preview().is_none());
    }

    #[test]
    fn esc_while_processing_clears_the_text() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        release(&mut c, 1.5);
        assert!(!c.view().live.is_empty());
        esc(&mut c, 1.8);
        assert!(c.view().live.is_empty());
    }

    #[test]
    fn a_short_discard_clears_the_text() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        shared.stop_len.set(3_200);
        release(&mut c, 1.5);
        let v = c.view();
        assert_eq!(v.phase, Phase::Idle);
        assert!(v.live.is_empty());
        assert_eq!(c.preview_wanted(), None);
    }

    #[test]
    fn a_stream_error_clears_the_text() {
        let (mut c, shared) = live_ready();
        start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        c.on_stream_error(CaptureError::Other("device lost".into()), 1.5);
        let v = c.view();
        assert_eq!(v.phase, Phase::Failed);
        assert!(v.live.is_empty());
        assert_eq!(c.preview_wanted(), None);
    }

    #[test]
    fn a_new_recording_starts_with_an_empty_preview() {
        let (mut c, shared) = live_ready();
        let first = start_live(&mut c);
        let job = first_pass(&mut c, &shared);
        show(&mut c, &job, "preview words here");
        c.on_stream_error(CaptureError::Other("device lost".into()), 1.5);
        release(&mut c, 1.6);
        c.poll(10.0);

        shared.captured.set(0);
        press(&mut c, 20.0);
        let second = c.take_capture().expect("capture id");
        assert_ne!(first, second);
        assert!(c.view().live.is_empty());
        assert_eq!(c.preview_wanted(), Some(second));
        let again = first_pass_at(&mut c, &shared, 21.0);
        assert_eq!(again.id, second);
    }

    fn first_pass_at(c: &mut Controller<FakeRecorder>, shared: &Shared, now: f64) -> PreviewJob {
        shared.captured.set(16_000);
        c.poll(now);
        c.take_preview().expect("preview job")
    }

    #[test]
    fn preview_wanted_is_none_outside_a_recording() {
        let (mut c, _s) = live_ready();
        assert_eq!(c.preview_wanted(), None);
        let id = start_live(&mut c);
        assert_eq!(c.preview_wanted(), Some(id));
        release(&mut c, 1.0);
        assert_eq!(c.preview_wanted(), None);
    }
}
