//! The controller loop body, free of Tauri and cpal so it runs in tests on any host.

use shepit_core::dictation::{Controller, Recorder, View};
use shepit_core::keys::RawKey;
use std::time::Instant;

/// What the hook and the recorder callback send to the controller thread.
#[derive(Debug, Clone, Copy)]
pub enum Msg {
    /// A raw keyboard hook event.
    Key(RawKey),
    /// A keyboard hook event stamped with `Instant::now()` inside the hook callback. The
    /// controller thread converts it with `Runner::step_at`.
    #[cfg_attr(not(windows), allow(dead_code))]
    KeyAt(RawKey, Instant),
    /// A raw (unsmoothed) level from the capture callback.
    Level(f32),
    /// The 100 ms receive timeout: lets the failed state reset.
    Tick,
}

/// Owns the dictation controller and reports when the widget view changed.
pub struct Runner<R: Recorder> {
    controller: Controller<R>,
    last: View,
}

impl<R: Recorder> Runner<R> {
    pub fn new(recorder: R) -> Self {
        let controller = Controller::new(recorder);
        let last = controller.view();
        Self { controller, last }
    }

    /// Feeds one message at `now` seconds on a monotonic clock. Returns the new view when it
    /// differs from the one returned last time, else None.
    pub fn step(&mut self, msg: Msg, now: f64) -> Option<View> {
        match msg {
            Msg::Key(raw) | Msg::KeyAt(raw, _) => self.controller.on_key(raw, now),
            Msg::Level(level) => self.controller.on_level(level),
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn take_capture(&mut self) -> Option<Vec<f32>> {
        self.controller.take_capture()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::dictation::{CaptureError, Phase};
    use shepit_core::keys::{LLKHF_EXTENDED, VK_RCONTROL, WM_KEYDOWN, WM_KEYUP};

    struct FakeRecorder;

    impl Recorder for FakeRecorder {
        fn start(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }
        fn stop(&mut self) -> Vec<f32> {
            vec![0.1; 16_000]
        }
    }

    fn rctrl(message: u32) -> Msg {
        Msg::Key(RawKey {
            message,
            vk: VK_RCONTROL,
            flags: LLKHF_EXTENDED,
        })
    }

    #[test]
    fn one_second_hold_goes_listening_then_idle() {
        let mut runner = Runner::new(FakeRecorder);
        let down = runner.step(rctrl(WM_KEYDOWN), 10.0).expect("view changes");
        assert_eq!(down.phase, Phase::Listening);
        assert!(!down.hands_free);

        let up = runner.step(rctrl(WM_KEYUP), 11.0).expect("view changes");
        assert_eq!(up.phase, Phase::Idle);
    }

    #[test]
    fn a_kept_hold_leaves_its_audio_for_the_next_task() {
        let mut runner = Runner::new(FakeRecorder);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        runner.step(rctrl(WM_KEYUP), 11.0);
        assert_eq!(runner.take_capture().map(|s| s.len()), Some(16_000));
    }

    #[test]
    fn a_tick_with_no_change_returns_none() {
        let mut runner = Runner::new(FakeRecorder);
        assert!(runner.step(Msg::Tick, 1.0).is_none());
    }

    #[test]
    fn levels_while_listening_move_the_bars() {
        let mut runner = Runner::new(FakeRecorder);
        runner.step(rctrl(WM_KEYDOWN), 10.0);
        let view = runner.step(Msg::Level(1.0), 10.1).expect("levels change");
        assert_eq!(view.levels.len(), 32);
        assert!(view.levels[31] > 0.0);
    }
}
