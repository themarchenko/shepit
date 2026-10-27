//! Timing rules shared by both agents (docs/spec/polishing.md, Process lifetime). Pure, with the
//! clock injected as seconds on any monotonic scale, so no test sleeps.

/// A request that gets no answer in this long fails (ClaudeCodeWorker.swift:12).
pub const REQUEST_TIMEOUT_SECS: f64 = 25.0;
/// Start, handshake and first turn (ClaudeCodeWorker.swift:13).
pub const WARM_UP_TIMEOUT_SECS: f64 = 40.0;
/// A conversation is replaced after this many turns (ClaudeCodeWorker.swift:10).
pub const MAX_TURNS: u32 = 25;
/// A standby conversation is prepared after this many turns (ClaudeCodeWorker.swift:11).
pub const STANDBY_AFTER_TURNS: u32 = 20;
/// Start failures in a row before a pause (ClaudeCodeWorker.swift:259).
pub const FAILURES_BEFORE_PAUSE: u32 = 2;
/// Pause length (ClaudeCodeWorker.swift:260).
pub const PAUSE_SECS: f64 = 300.0;
/// The agent is replaced this long after the last polishing setting change (AppDelegate.swift:170-188).
pub const DEBOUNCE_SECS: f64 = 1.5;

/// Counts failed starts and pauses starting after two in a row.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // fields are read once the logic lands
pub struct StartGate {
    failures: u32,
    paused_until: Option<f64>,
}

impl StartGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// `Ok` when a start may be tried at `now`. While paused: `Err(paused_until)`, so the format
    /// call fails at once and the dictation uses local text. The pause ends at `paused_until`.
    pub fn check(&self, now: f64) -> Result<(), f64> {
        let _ = now;
        todo!("task 9 chunk A: StartGate::check")
    }

    /// A start or warm-up failed at `now`. The second failure in a row starts the pause.
    pub fn on_start_failed(&mut self, now: f64) {
        let _ = now;
        todo!("task 9 chunk A: StartGate::on_start_failed")
    }

    /// A start succeeded: the count and the pause reset.
    pub fn on_start_ok(&mut self) {
        todo!("task 9 chunk A: StartGate::on_start_ok")
    }
}

/// The state of the second conversation prepared in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standby {
    Absent,
    Starting,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    Nothing,
    StartStandby,
    /// Close the active conversation and make the standby the active one.
    Promote,
}

/// What to do after a turn, given the turns the active conversation has had.
pub fn rotation(active_turns: u32, standby: Standby) -> Rotation {
    let _ = (active_turns, standby);
    todo!("task 9 chunk A: rotation")
}

/// Fires once, `delay` seconds after the last `touch`.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields are read once the logic lands
pub struct Debounce {
    delay: f64,
    last_touch: Option<f64>,
}

impl Debounce {
    pub fn new(delay: f64) -> Self {
        Self {
            delay,
            last_touch: None,
        }
    }

    pub fn touch(&mut self, now: f64) {
        let _ = now;
        todo!("task 9 chunk A: Debounce::touch")
    }

    /// True once when `delay` has passed since the last touch; false again until the next touch.
    pub fn due(&mut self, now: f64) -> bool {
        let _ = now;
        todo!("task 9 chunk A: Debounce::due")
    }

    pub fn pending(&self) -> bool {
        todo!("task 9 chunk A: Debounce::pending")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_match_the_swift_agents() {
        assert_eq!(REQUEST_TIMEOUT_SECS, 25.0);
        assert_eq!(WARM_UP_TIMEOUT_SECS, 40.0);
        assert_eq!(MAX_TURNS, 25);
        assert_eq!(STANDBY_AFTER_TURNS, 20);
        assert_eq!(FAILURES_BEFORE_PAUSE, 2);
        assert_eq!(PAUSE_SECS, 300.0);
        assert_eq!(DEBOUNCE_SECS, 1.5);
    }

    #[test]
    fn a_fresh_gate_lets_a_start_through() {
        assert_eq!(StartGate::new().check(0.0), Ok(()));
    }

    #[test]
    fn one_failed_start_does_not_pause() {
        let mut g = StartGate::new();
        g.on_start_failed(10.0);
        assert_eq!(g.check(10.1), Ok(()));
    }

    #[test]
    fn two_failed_starts_in_a_row_pause_for_300_seconds() {
        let mut g = StartGate::new();
        g.on_start_failed(10.0);
        g.on_start_failed(20.0);
        assert_eq!(g.check(20.0), Err(320.0));
        assert_eq!(g.check(319.9), Err(320.0));
        assert_eq!(g.check(320.0), Ok(()));
    }

    #[test]
    fn a_call_while_paused_fails_at_once_every_time() {
        let mut g = StartGate::new();
        g.on_start_failed(0.0);
        g.on_start_failed(1.0);
        for now in [1.0, 50.0, 200.0] {
            assert!(g.check(now).is_err(), "{now}");
        }
    }

    #[test]
    fn a_success_resets_the_count() {
        let mut g = StartGate::new();
        g.on_start_failed(0.0);
        g.on_start_ok();
        g.on_start_failed(1.0);
        assert_eq!(g.check(1.0), Ok(()), "the failures were not in a row");
    }

    #[test]
    fn a_success_clears_a_pause() {
        let mut g = StartGate::new();
        g.on_start_failed(0.0);
        g.on_start_failed(1.0);
        g.on_start_ok();
        assert_eq!(g.check(2.0), Ok(()));
    }

    #[test]
    fn a_failure_after_the_pause_pauses_again() {
        let mut g = StartGate::new();
        g.on_start_failed(0.0);
        g.on_start_failed(1.0);
        assert_eq!(g.check(301.0), Ok(()));
        g.on_start_failed(301.0);
        assert_eq!(g.check(302.0), Err(601.0));
    }

    #[test]
    fn no_standby_is_prepared_before_20_turns() {
        assert_eq!(rotation(0, Standby::Absent), Rotation::Nothing);
        assert_eq!(rotation(19, Standby::Absent), Rotation::Nothing);
    }

    #[test]
    fn the_standby_starts_at_20_turns() {
        assert_eq!(rotation(20, Standby::Absent), Rotation::StartStandby);
        assert_eq!(rotation(22, Standby::Absent), Rotation::StartStandby);
    }

    #[test]
    fn a_standby_already_starting_or_ready_is_not_started_again() {
        assert_eq!(rotation(20, Standby::Starting), Rotation::Nothing);
        assert_eq!(rotation(20, Standby::Ready), Rotation::Nothing);
        assert_eq!(rotation(24, Standby::Ready), Rotation::Nothing);
    }

    #[test]
    fn the_standby_is_promoted_at_25_turns_only_when_ready() {
        assert_eq!(rotation(25, Standby::Ready), Rotation::Promote);
        assert_eq!(rotation(26, Standby::Ready), Rotation::Promote);
        assert_eq!(rotation(25, Standby::Starting), Rotation::Nothing);
    }

    #[test]
    fn a_missing_standby_at_25_turns_is_started_not_promoted() {
        assert_eq!(rotation(25, Standby::Absent), Rotation::StartStandby);
    }

    #[test]
    fn a_debounce_is_idle_until_touched() {
        let mut d = Debounce::new(DEBOUNCE_SECS);
        assert!(!d.pending());
        assert!(!d.due(100.0));
    }

    #[test]
    fn a_debounce_fires_1_5_seconds_after_the_touch() {
        let mut d = Debounce::new(DEBOUNCE_SECS);
        d.touch(10.0);
        assert!(d.pending());
        assert!(!d.due(11.4));
        assert!(d.due(11.5));
    }

    #[test]
    fn a_debounce_fires_once() {
        let mut d = Debounce::new(DEBOUNCE_SECS);
        d.touch(0.0);
        assert!(d.due(2.0));
        assert!(!d.due(3.0));
        assert!(!d.pending());
    }

    #[test]
    fn every_touch_restarts_the_wait() {
        let mut d = Debounce::new(DEBOUNCE_SECS);
        d.touch(0.0);
        d.touch(1.0);
        d.touch(2.0);
        assert!(!d.due(3.4));
        assert!(d.due(3.5));
    }

    #[test]
    fn a_debounce_can_be_used_again_after_firing() {
        let mut d = Debounce::new(DEBOUNCE_SECS);
        d.touch(0.0);
        assert!(d.due(2.0));
        d.touch(5.0);
        assert!(!d.due(6.0));
        assert!(d.due(6.5));
    }
}
