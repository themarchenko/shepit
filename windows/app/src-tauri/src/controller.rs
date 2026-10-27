//! The controller loop body, free of Tauri and cpal so it runs in tests on any host.

use shepit_core::context::TargetContext;
use shepit_core::dictation::{
    CaptureError, Controller, InsertJob, Job, ModelStatus, Recorder, View, WidgetInput,
};
use shepit_core::insert::InsertOutcome;
use shepit_core::keys::{Hotkey, HotkeyPicker, PickResult, RawKey};
use std::time::Instant;

/// What the hook and the recorder callback send to the controller thread.
#[derive(Debug, Clone)]
pub enum Msg {
    /// A raw keyboard hook event.
    Key(RawKey),
    /// A keyboard hook event stamped with `Instant::now()` inside the hook callback. The
    /// controller thread converts it with `Runner::step_at`.
    #[cfg_attr(not(windows), allow(dead_code))]
    KeyAt(RawKey, Instant),
    /// A raw (unsmoothed) level from the capture callback.
    Level(f32),
    /// The capture stream reported an error while recording.
    StreamError(CaptureError),
    /// The recognizer's download and load state.
    Model(ModelStatus),
    /// The recognizer finished (or gave up on) job `id`.
    Recognized {
        id: u64,
        result: Result<String, String>,
    },
    /// The insert worker finished dictation `id`.
    Inserted {
        id: u64,
        result: Result<InsertOutcome, String>,
    },
    /// The insert worker read the target context for dictation `id`.
    Context { id: u64, context: TargetContext },
    /// A click or hold on the widget.
    Widget(WidgetInput),
    /// The dictation key changed (a settings write). Drops a half-finished press of the old key.
    Hotkey(Hotkey),
    /// Settings starts or cancels the key picker.
    Pick(PickMsg),
    /// The 100 ms receive timeout: lets the failed state reset.
    Tick,
}

/// What Settings asks of the key picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMsg {
    /// Divert the keys to the picker until it ends.
    Start,
    /// The window closed or lost focus: end the pick as `Cancelled`.
    Cancel,
}

/// Owns the dictation controller and reports when the widget view changed.
pub struct Runner<R: Recorder> {
    controller: Controller<R>,
    last: View,
    /// Set while keys go to the picker instead of dictation.
    picker: Option<HotkeyPicker>,
    /// How the last pick ended, until `take_pick` hands it over.
    pick: Option<PickResult>,
}

impl<R: Recorder> Runner<R> {
    /// `first_job_id` is where this controller's job ids start (see `Controller::with_first_job_id`).
    /// `model` is the latest status the recognizer reported: it sends Ready once, so a controller
    /// restarted later has to be told, or every dictation would wait for a Ready that never comes.
    pub fn new(recorder: R, first_job_id: u64, model: ModelStatus) -> Self {
        let mut controller = Controller::new(recorder).with_first_job_id(first_job_id);
        controller.on_model(model);
        let last = controller.view();
        Self {
            controller,
            last,
            picker: None,
            pick: None,
        }
    }

    /// Feeds one message at `now` seconds on a monotonic clock. Returns the new view when it
    /// differs from the one returned last time, else None.
    pub fn step(&mut self, msg: Msg, now: f64) -> Option<View> {
        match msg {
            // While picking, no key reaches dictation: the picker takes the candidates and Esc.
            Msg::Key(raw) | Msg::KeyAt(raw, _) => match self.picker.as_mut() {
                Some(picker) => {
                    if let Some(result) = picker.on_key(raw) {
                        self.end_pick(result);
                    }
                }
                None => self.controller.on_key(raw, now),
            },
            Msg::Level(level) => self.controller.on_level(level),
            Msg::StreamError(err) => self.controller.on_stream_error(err, now),
            Msg::Model(status) => self.controller.on_model(status),
            Msg::Recognized { id, result } => self.controller.on_recognized(id, result, now),
            Msg::Context { id, context } => self.controller.on_context(id, context),
            Msg::Inserted { id, result } => self.controller.on_inserted(id, result, now),
            Msg::Widget(input) => self.controller.on_widget(input, now),
            Msg::Hotkey(hotkey) => self.controller.set_hotkey(hotkey),
            Msg::Pick(PickMsg::Start) => {
                self.pick = None;
                self.picker = Some(HotkeyPicker::new(now));
            }
            Msg::Pick(PickMsg::Cancel) => {
                if self.picker.is_some() {
                    self.end_pick(PickResult::Cancelled);
                }
            }
            Msg::Tick => self.controller.poll(now),
        }
        // Every step checks the clock, so the 15 s limit holds even while keys keep arriving.
        if let Some(result) = self.picker.as_mut().and_then(|p| p.poll(now)) {
            self.end_pick(result);
        }
        let view = self.controller.view();
        if view == self.last {
            None
        } else {
            self.last = view.clone();
            Some(view)
        }
    }

    /// Like `step`, but a `KeyAt` event is timed by when the hook saw it, not when this thread
    /// got to it. `epoch` is the zero of the monotonic clock in seconds.
    pub fn step_at(&mut self, msg: Msg, epoch: Instant, now: Instant) -> Option<View> {
        let secs = |at: Instant| at.saturating_duration_since(epoch).as_secs_f64();
        match msg {
            Msg::KeyAt(raw, at) => self.step(Msg::Key(raw), secs(at)),
            other => self.step(other, secs(now)),
        }
    }

    fn end_pick(&mut self, result: PickResult) {
        self.picker = None;
        self.pick = Some(result);
    }

    /// Starts the controller on `hotkey` (the app reads it from settings, or from the hook after a
    /// controller restart).
    pub fn with_hotkey(mut self, hotkey: Hotkey) -> Self {
        self.controller.set_hotkey(hotkey);
        self
    }

    /// True while keys go to the picker instead of dictation.
    pub fn picking(&self) -> bool {
        self.picker.is_some()
    }

    /// How the last pick ended, handed over once. Set by a candidate key, Esc, a Cancel message
    /// or the 15 s timeout (checked on every step, so a Tick is enough).
    pub fn take_pick(&mut self) -> Option<PickResult> {
        self.pick.take()
    }

    /// The job created by the last stop, handed over once.
    pub fn take_job(&mut self) -> Option<Job> {
        self.controller.take_job()
    }

    /// The id of the job whose result is still wanted, or 0. Job ids start at 1.
    pub fn wanted(&self) -> u64 {
        self.controller.current_job().unwrap_or(0)
    }

    /// The dictation id to capture the target context for, handed over once.
    pub fn take_capture(&mut self) -> Option<u64> {
        self.controller.take_capture()
    }

    /// The accepted transcript as an insert job, handed over once.
    pub fn take_insert(&mut self) -> Option<InsertJob> {
        self.controller.take_insert()
    }
}

/// The log line for an accepted transcript. Release builds pass `include_text` false: a
/// transcript is the user's speech and stays out of the log. Debug builds log 80 characters.
pub fn log_line(text: &str, include_text: bool) -> Option<String> {
    if !include_text {
        return None;
    }
    let shown: String = text
        .chars()
        .take(LOG_TEXT_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    Some(format!("transcript: {shown}"))
}

/// Debug builds log this many characters of a transcript (docs/spec/polishing.md:76).
const LOG_TEXT_CHARS: usize = 80;

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::dictation::{CaptureError, Phase, Stage};
    use shepit_core::keys::{
        LLKHF_EXTENDED, VK_ESCAPE, VK_RCONTROL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    };

    struct FakeRecorder;

    impl Recorder for FakeRecorder {
        fn start(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }
        fn stop(&mut self) -> Vec<f32> {
            // Half silence so the speech gate passes (a constant tone has no energy step).
            let mut v = vec![0.0; 8_000];
            v.extend(vec![0.1; 8_000]);
            v
        }
    }

    fn rctrl(message: u32) -> Msg {
        Msg::Key(RawKey {
            message,
            vk: VK_RCONTROL,
            flags: LLKHF_EXTENDED,
        })
    }

    fn esc() -> Msg {
        Msg::Key(RawKey {
            message: WM_KEYDOWN,
            vk: VK_ESCAPE,
            flags: 0,
        })
    }

    /// Records a one second hold with the model ready; returns the runner in Transcribing.
    fn transcribing() -> Runner<FakeRecorder> {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        runner.step(Msg::Model(ModelStatus::Ready), 9.0);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let up = runner.step(rctrl(WM_KEYUP), 11.0).expect("view changes");
        assert_eq!(up.phase, Phase::Processing);
        runner
    }

    #[test]
    fn one_second_hold_goes_listening_then_transcribing() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        runner.step(Msg::Model(ModelStatus::Ready), 9.0);
        let down = runner.step(rctrl(WM_KEYDOWN), 10.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
        assert!(!down.hands_free);

        let up = runner.step(rctrl(WM_KEYUP), 11.0).expect("view changes");
        assert_eq!(up.phase, Phase::Processing);
        assert_eq!(up.stage, Some(Stage::Transcribing));
    }

    #[test]
    fn a_kept_hold_hands_out_one_job_and_wants_its_id() {
        let mut runner = transcribing();
        assert_eq!(runner.wanted(), 1);
        let job = runner.take_job().expect("a job");
        assert_eq!(job.id, 1);
        assert_eq!(job.samples.len(), 16_000);
        assert!(runner.take_job().is_none());
    }

    #[test]
    fn first_job_id_is_the_one_given() {
        let mut runner = Runner::new(FakeRecorder, 1 << 32, ModelStatus::Loading);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        runner.step(rctrl(WM_KEYUP), 11.0);
        assert_eq!(runner.take_job().map(|j| j.id), Some(1 << 32));
    }

    #[test]
    fn a_result_for_the_job_finishes_it_and_hands_out_the_text() {
        let mut runner = transcribing();
        let inserting = runner
            .step(
                Msg::Recognized {
                    id: 1,
                    result: Ok("hello there".into()),
                },
                12.0,
            )
            .expect("view changes");
        assert_eq!(inserting.stage, Some(Stage::Inserting));
        assert_eq!(runner.wanted(), 0);
        assert_eq!(
            runner.take_insert().map(|j| j.text).as_deref(),
            Some("hello there")
        );
        assert!(runner.take_insert().is_none());
        let done = runner
            .step(
                Msg::Inserted {
                    id: 1,
                    result: Ok(InsertOutcome::Pasted),
                },
                12.5,
            )
            .expect("view changes");
        assert_eq!(done.phase, Phase::Done);
        assert_eq!(done.message, "hello there");
    }

    #[test]
    fn a_stale_result_is_dropped() {
        let mut runner = transcribing();
        let stale = Msg::Recognized {
            id: 1 << 32,
            result: Ok("old".into()),
        };
        assert!(runner.step(stale, 12.0).is_none());
        assert_eq!(runner.wanted(), 1);
        assert!(runner.take_insert().is_none());
    }

    #[test]
    fn esc_while_transcribing_goes_idle_and_the_late_result_is_ignored() {
        let mut runner = transcribing();
        let idle = runner.step(esc(), 11.5).expect("view changes");
        assert_eq!(idle.phase, Phase::Idle);
        assert_eq!(runner.wanted(), 0);
        let late = Msg::Recognized {
            id: 1,
            result: Ok("late".into()),
        };
        assert!(runner.step(late, 12.0).is_none());
        assert!(runner.take_insert().is_none());
    }

    #[test]
    fn a_recognizer_error_fails_the_view_with_its_text() {
        let mut runner = transcribing();
        let failed = runner
            .step(
                Msg::Recognized {
                    id: 1,
                    result: Err("Recognizer is not running".into()),
                },
                12.0,
            )
            .expect("view changes");
        assert_eq!(failed.phase, Phase::Failed);
        assert_eq!(failed.message, "Recognizer is not running");
    }

    #[test]
    fn a_dictation_before_the_model_waits_and_shows_the_download() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let waiting = runner.step(rctrl(WM_KEYUP), 11.0).expect("view changes");
        assert_eq!(waiting.stage, Some(Stage::Loading));
        let p = runner
            .step(Msg::Model(ModelStatus::Downloading(0.42)), 11.2)
            .expect("progress shows");
        assert_eq!(p.progress, Some(0.42));
        let ready = runner
            .step(Msg::Model(ModelStatus::Ready), 12.0)
            .expect("view changes");
        assert_eq!(ready.stage, Some(Stage::Transcribing));
        assert_eq!(ready.progress, None);
    }

    #[test]
    fn a_restarted_controller_starts_from_the_known_model_status() {
        // The recognizer reports Ready once; a controller restarted after that must not wait for it.
        let mut runner = Runner::new(FakeRecorder, 1 << 32, ModelStatus::Ready);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let up = runner.step(rctrl(WM_KEYUP), 11.0).expect("view changes");
        assert_eq!(up.stage, Some(Stage::Transcribing));
    }

    #[test]
    fn model_status_while_idle_changes_nothing() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        assert!(runner
            .step(Msg::Model(ModelStatus::Downloading(0.5)), 1.0)
            .is_none());
    }

    #[test]
    fn log_line_is_none_when_text_is_not_allowed() {
        assert_eq!(log_line("secret words", false), None);
    }

    #[test]
    fn log_line_keeps_at_most_80_characters_of_text() {
        let long = "ж".repeat(200);
        let line = log_line(&long, true).expect("a line");
        assert_eq!(line.matches('ж').count(), 80);
        let short = log_line("hi\nthere", true).expect("a line");
        assert!(short.ends_with("hi there"), "{short}");
    }

    #[test]
    fn a_stream_error_while_listening_fails_the_view() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let view = runner
            .step(Msg::StreamError(CaptureError::Other("lost".into())), 10.5)
            .expect("view changes");
        assert_eq!(view.phase, Phase::Failed);
        assert_eq!(view.message, "Microphone: lost");
    }

    #[test]
    fn a_tick_with_no_change_returns_none() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        assert!(runner.step(Msg::Tick, 1.0).is_none());
    }

    #[test]
    fn levels_while_listening_move_the_bars() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let view = runner.step(Msg::Level(1.0), 10.1).expect("levels change");
        assert_eq!(view.levels.len(), 32);
        assert!(view.levels[31] > 0.0);
    }

    fn password_context(flag: Option<bool>) -> TargetContext {
        TargetContext {
            password: flag,
            ..TargetContext::default()
        }
    }

    #[test]
    fn a_start_hands_out_one_capture_with_the_dictation_id() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Ready);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        assert_eq!(runner.take_capture(), Some(1));
        assert_eq!(runner.take_capture(), None);
    }

    #[test]
    fn a_context_message_reaches_the_insert_job() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Ready);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let id = runner.take_capture().expect("capture");
        runner.step(
            Msg::Context {
                id,
                context: password_context(Some(true)),
            },
            10.5,
        );
        runner.step(rctrl(WM_KEYUP), 11.0);
        runner.step(
            Msg::Recognized {
                id,
                result: Ok("secret".into()),
            },
            12.0,
        );
        let job = runner.take_insert().expect("insert job");
        assert_eq!(job.captured_password, Some(true));
    }

    #[test]
    fn a_context_for_an_older_dictation_is_dropped() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Ready);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let id = runner.take_capture().expect("capture");
        runner.step(
            Msg::Context {
                id: id + 100,
                context: password_context(Some(true)),
            },
            10.5,
        );
        runner.step(rctrl(WM_KEYUP), 11.0);
        runner.step(
            Msg::Recognized {
                id,
                result: Ok("text".into()),
            },
            12.0,
        );
        assert_eq!(runner.take_insert().unwrap().captured_password, None);
    }

    #[test]
    fn a_clipboard_only_result_shows_the_ctrl_v_hint() {
        let mut runner = transcribing();
        runner.step(
            Msg::Recognized {
                id: 1,
                result: Ok("hello there".into()),
            },
            12.0,
        );
        let done = runner
            .step(
                Msg::Inserted {
                    id: 1,
                    result: Ok(InsertOutcome::ClipboardOnly),
                },
                12.5,
            )
            .expect("view changes");
        assert_eq!(done.phase, Phase::Done);
        assert!(done.message.starts_with("Copied (press Ctrl+V to paste)"));
    }

    #[test]
    fn an_insert_error_fails_the_view_with_its_message() {
        let mut runner = transcribing();
        runner.step(
            Msg::Recognized {
                id: 1,
                result: Ok("hello".into()),
            },
            12.0,
        );
        let failed = runner
            .step(
                Msg::Inserted {
                    id: 1,
                    result: Err("Insertion needs Windows".into()),
                },
                12.5,
            )
            .expect("view changes");
        assert_eq!(failed.phase, Phase::Failed);
        assert_eq!(failed.message, "Insertion needs Windows");
    }

    // The dictation key and the picker.

    fn ready_runner() -> Runner<FakeRecorder> {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Loading);
        runner.step(Msg::Model(ModelStatus::Ready), 9.0);
        runner
    }

    fn key_down(vk: u32, flags: u32) -> Msg {
        Msg::Key(RawKey {
            message: WM_KEYDOWN,
            vk,
            flags,
        })
    }

    fn alt(message: u32) -> Msg {
        Msg::Key(RawKey {
            message,
            vk: 0xA5,
            flags: LLKHF_EXTENDED,
        })
    }

    /// A double tap of Right Ctrl: the recording goes hands-free.
    fn hands_free_runner() -> Runner<FakeRecorder> {
        let mut runner = ready_runner();
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        runner.step(rctrl(WM_KEYUP), 10.1);
        runner.step(rctrl(WM_KEYDOWN), 10.2);
        let v = runner.step(rctrl(WM_KEYUP), 10.3).expect("view changes");
        assert!(v.hands_free, "setup: expected a hands-free recording");
        runner
    }

    #[test]
    fn a_hotkey_message_switches_the_key() {
        let mut runner = ready_runner();
        runner.step(Msg::Hotkey(Hotkey::RightAlt), 10.0);
        // The old key does nothing.
        assert!(runner.step(rctrl(WM_KEYDOWN), 11.0).is_none());
        assert!(runner.step(rctrl(WM_KEYUP), 11.5).is_none());
        // The new key records.
        let down = runner.step(alt(WM_KEYDOWN), 12.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
        let up = runner.step(alt(WM_KEYUP), 13.0).expect("view changes");
        assert_eq!(up.phase, Phase::Processing);
    }

    #[test]
    fn a_hotkey_message_drops_a_half_finished_press() {
        let mut runner = ready_runner();
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        runner.step(esc(), 10.5);
        runner.step(Msg::Hotkey(Hotkey::RightAlt), 11.0);
        let down = runner.step(alt(WM_KEYDOWN), 12.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
    }

    #[test]
    fn a_runner_built_with_a_hotkey_starts_on_it() {
        let mut runner = Runner::new(FakeRecorder, 1, ModelStatus::Ready).with_hotkey(Hotkey::F13);
        assert!(runner.step(rctrl(WM_KEYDOWN), 10.0).is_none());
        runner.step(rctrl(WM_KEYUP), 10.5);
        let down = runner.step(key_down(0x7C, 0), 11.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
    }

    #[test]
    fn picking_is_off_until_it_starts() {
        let mut runner = ready_runner();
        assert!(!runner.picking());
        assert_eq!(runner.take_pick(), None);
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        assert!(runner.picking());
    }

    #[test]
    fn a_pick_start_changes_no_view() {
        let mut runner = ready_runner();
        assert!(runner.step(Msg::Pick(PickMsg::Start), 10.0).is_none());
    }

    #[test]
    fn right_ctrl_while_picking_is_picked_and_the_phase_stays_idle() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        let view = runner.step(rctrl(WM_KEYDOWN), 11.0);
        assert!(view.is_none(), "no recording starts: {view:?}");
        assert_eq!(
            runner.take_pick(),
            Some(PickResult::Picked(Hotkey::RightCtrl))
        );
        assert_eq!(runner.take_pick(), None);
        assert!(!runner.picking());
        // The release that follows is not a dictation either.
        assert!(runner.step(rctrl(WM_KEYUP), 11.2).is_none());
    }

    #[test]
    fn right_alt_and_function_keys_are_picked() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(alt(WM_SYSKEYDOWN), 11.0);
        assert_eq!(
            runner.take_pick(),
            Some(PickResult::Picked(Hotkey::RightAlt))
        );
        runner.step(Msg::Pick(PickMsg::Start), 12.0);
        runner.step(key_down(0x86, 0), 13.0);
        assert_eq!(runner.take_pick(), Some(PickResult::Picked(Hotkey::F23)));
    }

    #[test]
    fn a_timestamped_hook_event_is_picked_too() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        let raw = RawKey {
            message: WM_KEYDOWN,
            vk: 0x7C,
            flags: 0,
        };
        let epoch = Instant::now();
        runner.step_at(Msg::KeyAt(raw, Instant::now()), epoch, Instant::now());
        assert_eq!(runner.take_pick(), Some(PickResult::Picked(Hotkey::F13)));
    }

    #[test]
    fn other_keys_while_picking_are_ignored_and_the_pick_goes_on() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(key_down(0x41, 0), 10.5);
        runner.step(key_down(0xA2, 0), 10.6);
        assert_eq!(runner.take_pick(), None);
        assert!(runner.picking());
    }

    #[test]
    fn picking_does_not_change_the_key_by_itself() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(alt(WM_KEYDOWN), 11.0);
        runner.step(alt(WM_KEYUP), 11.1);
        assert_eq!(
            runner.take_pick(),
            Some(PickResult::Picked(Hotkey::RightAlt))
        );
        // The app applies the result with Msg::Hotkey; until then Right Ctrl still records.
        let down = runner.step(rctrl(WM_KEYDOWN), 12.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
    }

    #[test]
    fn esc_while_picking_cancels_the_pick_and_leaves_a_hands_free_recording_running() {
        let mut runner = hands_free_runner();
        runner.step(Msg::Pick(PickMsg::Start), 11.0);
        let view = runner.step(esc(), 12.0);
        assert!(view.is_none(), "the recording must not change: {view:?}");
        assert_eq!(runner.take_pick(), Some(PickResult::Cancelled));
        assert!(!runner.picking());
        // Still listening: a later Esc, outside the picker, ends it.
        let cancelled = runner.step(esc(), 13.0).expect("view changes");
        assert_eq!(cancelled.phase, Phase::Idle);
    }

    #[test]
    fn a_cancel_message_ends_the_pick_as_cancelled() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(Msg::Pick(PickMsg::Cancel), 11.0);
        assert_eq!(runner.take_pick(), Some(PickResult::Cancelled));
        assert!(!runner.picking());
    }

    #[test]
    fn a_cancel_message_without_a_pick_gives_nothing() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Cancel), 10.0);
        assert_eq!(runner.take_pick(), None);
    }

    #[test]
    fn picking_times_out_on_a_tick_after_15_seconds() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(Msg::Tick, 24.9);
        assert_eq!(runner.take_pick(), None);
        assert!(runner.picking());
        runner.step(Msg::Tick, 25.0);
        assert_eq!(runner.take_pick(), Some(PickResult::TimedOut));
        assert!(!runner.picking());
    }

    #[test]
    fn keys_go_back_to_dictation_after_a_pick() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(rctrl(WM_KEYDOWN), 11.0);
        runner.step(rctrl(WM_KEYUP), 11.5);
        assert!(runner.take_pick().is_some());
        let down = runner.step(rctrl(WM_KEYDOWN), 12.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
    }

    #[test]
    fn keys_go_back_to_dictation_after_a_cancel_and_after_a_timeout() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(esc(), 11.0);
        assert!(runner.take_pick().is_some());
        let down = runner.step(rctrl(WM_KEYDOWN), 12.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
        runner.step(rctrl(WM_KEYUP), 12.1);
        runner.step(esc(), 12.5);

        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(Msg::Tick, 26.0);
        assert!(runner.take_pick().is_some());
        let down = runner.step(rctrl(WM_KEYDOWN), 27.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
    }

    #[test]
    fn a_second_pick_can_follow_the_first() {
        let mut runner = ready_runner();
        runner.step(Msg::Pick(PickMsg::Start), 10.0);
        runner.step(key_down(0x7C, 0), 11.0);
        assert!(runner.take_pick().is_some());
        runner.step(Msg::Pick(PickMsg::Start), 20.0);
        runner.step(key_down(0x7D, 0), 21.0);
        assert_eq!(runner.take_pick(), Some(PickResult::Picked(Hotkey::F14)));
    }
}
