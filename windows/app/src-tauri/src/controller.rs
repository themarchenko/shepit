//! The controller loop body, free of Tauri and cpal so it runs in tests on any host.

use shepit_core::dictation::{
    CaptureError, Controller, InsertJob, Job, ModelStatus, Recorder, View,
};
use shepit_core::insert::InsertOutcome;
use shepit_core::keys::RawKey;
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
    /// The 100 ms receive timeout: lets the failed state reset.
    Tick,
}

/// Owns the dictation controller and reports when the widget view changed.
pub struct Runner<R: Recorder> {
    controller: Controller<R>,
    last: View,
}

impl<R: Recorder> Runner<R> {
    /// `first_job_id` is where this controller's job ids start (see `Controller::with_first_job_id`).
    /// `model` is the latest status the recognizer reported: it sends Ready once, so a controller
    /// restarted later has to be told, or every dictation would wait for a Ready that never comes.
    pub fn new(recorder: R, first_job_id: u64, model: ModelStatus) -> Self {
        let mut controller = Controller::new(recorder).with_first_job_id(first_job_id);
        controller.on_model(model);
        let last = controller.view();
        Self { controller, last }
    }

    /// Feeds one message at `now` seconds on a monotonic clock. Returns the new view when it
    /// differs from the one returned last time, else None.
    pub fn step(&mut self, msg: Msg, now: f64) -> Option<View> {
        match msg {
            Msg::Key(raw) | Msg::KeyAt(raw, _) => self.controller.on_key(raw, now),
            Msg::Level(level) => self.controller.on_level(level),
            Msg::StreamError(err) => self.controller.on_stream_error(err, now),
            Msg::Model(status) => self.controller.on_model(status),
            Msg::Recognized { id, result } => self.controller.on_recognized(id, result, now),
            Msg::Inserted { id, result } => self.controller.on_inserted(id, result, now),
            Msg::Tick => self.controller.poll(now),
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

    /// The job created by the last stop, handed over once.
    pub fn take_job(&mut self) -> Option<Job> {
        self.controller.take_job()
    }

    /// The id of the job whose result is still wanted, or 0. Job ids start at 1.
    pub fn wanted(&self) -> u64 {
        self.controller.current_job().unwrap_or(0)
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
    use shepit_core::keys::{LLKHF_EXTENDED, VK_ESCAPE, VK_RCONTROL, WM_KEYDOWN, WM_KEYUP};

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
}
