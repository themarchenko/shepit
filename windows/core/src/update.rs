//! Update check schedule and result handling. Mirrors `UpdateChecker.swift:22-75`; the rules are
//! written down in docs/spec/updates-install.md ("Check schedule").

/// Delay before the first look after launch.
pub const FIRST_LOOK_SECS: u64 = 20;
/// Gap between looks, so a machine that sleeps overnight still checks.
pub const LOOK_EVERY_SECS: u64 = 3600;
/// A look asks GitHub only when the last successful check is at least this old.
pub const CHECK_INTERVAL_SECS: u64 = 86_400;

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(String),
    /// Version and fraction done, 0.0 to 1.0.
    Downloading(String, f64),
    Installing(String),
    Failed(String),
    /// Version and message: the download or install failed after the user chose to install.
    InstallFailed(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckResult {
    UpToDate,
    /// A release newer than the installed version, by its version string.
    Newer(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckOutcome {
    pub state: UpdateState,
    /// Unix seconds to store as `lastUpdateCheck`; `None` leaves the stored time alone.
    pub store_check_time: Option<u64>,
    /// Whether the update card opens.
    pub show_card: bool,
}

/// A look asks GitHub only when checking is on and the last successful check is a day old.
pub fn is_due(enabled: bool, last_check: Option<u64>, now: u64) -> bool {
    if !enabled {
        return false;
    }
    match last_check {
        None => true,
        // A last check in the future (clock set back) is not due, or every look would ask.
        Some(last) => now >= last && now - last >= CHECK_INTERVAL_SECS,
    }
}

/// A user's own check always shows the result; a background check skips the version the user
/// chose to skip.
pub fn should_offer(user_initiated: bool, version: &str, skipped: &str) -> bool {
    user_initiated || version != skipped
}

pub fn on_check_result(
    result: CheckResult,
    user_initiated: bool,
    skipped: &str,
    now: u64,
) -> CheckOutcome {
    match result {
        CheckResult::UpToDate => CheckOutcome {
            state: UpdateState::UpToDate,
            store_check_time: Some(now),
            show_card: user_initiated,
        },
        CheckResult::Newer(version) => {
            let show_card = should_offer(user_initiated, &version, skipped);
            CheckOutcome {
                state: UpdateState::Available(version),
                store_check_time: Some(now),
                show_card,
            }
        }
        // A failed check stores nothing, so the next look retries. Only the user sees the error.
        CheckResult::Error(message) => CheckOutcome {
            state: if user_initiated {
                UpdateState::Failed(message)
            } else {
                UpdateState::Idle
            },
            store_check_time: None,
            show_card: user_initiated,
        },
    }
}

/// Nothing is checked while a check, download or install runs.
pub fn is_busy(state: &UpdateState) -> bool {
    matches!(
        state,
        UpdateState::Checking | UpdateState::Downloading(..) | UpdateState::Installing(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;
    const NOW: u64 = 1_800_000_000;

    #[test]
    fn schedule_constants_match_the_spec() {
        assert_eq!(FIRST_LOOK_SECS, 20);
        assert_eq!(LOOK_EVERY_SECS, 3600);
        assert_eq!(CHECK_INTERVAL_SECS, DAY);
    }

    #[test]
    fn not_due_when_checking_is_off() {
        assert!(!is_due(false, None, NOW));
        assert!(!is_due(false, Some(NOW - 2 * DAY), NOW));
    }

    #[test]
    fn due_when_never_checked() {
        assert!(is_due(true, None, NOW));
    }

    #[test]
    fn not_due_one_second_before_a_day() {
        assert!(!is_due(true, Some(NOW - (DAY - 1)), NOW));
    }

    #[test]
    fn due_after_exactly_a_day() {
        assert!(is_due(true, Some(NOW - DAY), NOW));
    }

    #[test]
    fn not_due_when_the_last_check_is_in_the_future() {
        // A clock set back must not trigger a request on every look.
        assert!(!is_due(true, Some(NOW + 10), NOW));
    }

    #[test]
    fn a_user_check_always_offers() {
        assert!(should_offer(true, "0.4.0", "0.4.0"));
    }

    #[test]
    fn a_background_check_does_not_offer_the_skipped_version() {
        assert!(!should_offer(false, "0.4.0", "0.4.0"));
    }

    #[test]
    fn a_background_check_offers_another_version() {
        assert!(should_offer(false, "0.4.1", "0.4.0"));
    }

    #[test]
    fn a_background_check_offers_when_nothing_is_skipped() {
        assert!(should_offer(false, "0.4.0", ""));
    }

    #[test]
    fn up_to_date_stores_the_time_and_stays_quiet_in_the_background() {
        let out = on_check_result(CheckResult::UpToDate, false, "", NOW);
        assert_eq!(out.state, UpdateState::UpToDate);
        assert_eq!(out.store_check_time, Some(NOW));
        assert!(!out.show_card);
    }

    #[test]
    fn up_to_date_shows_the_card_for_a_user_check() {
        let out = on_check_result(CheckResult::UpToDate, true, "", NOW);
        assert_eq!(out.state, UpdateState::UpToDate);
        assert_eq!(out.store_check_time, Some(NOW));
        assert!(out.show_card);
    }

    #[test]
    fn a_newer_release_stores_the_time_and_is_offered() {
        let out = on_check_result(CheckResult::Newer("0.4.0".into()), false, "", NOW);
        assert_eq!(out.state, UpdateState::Available("0.4.0".into()));
        assert_eq!(out.store_check_time, Some(NOW));
        assert!(out.show_card);
    }

    #[test]
    fn a_skipped_release_is_available_but_not_shown_in_the_background() {
        let out = on_check_result(CheckResult::Newer("0.4.0".into()), false, "0.4.0", NOW);
        assert_eq!(out.state, UpdateState::Available("0.4.0".into()));
        assert_eq!(out.store_check_time, Some(NOW));
        assert!(!out.show_card);
    }

    #[test]
    fn a_skipped_release_is_shown_when_the_user_asked() {
        let out = on_check_result(CheckResult::Newer("0.4.0".into()), true, "0.4.0", NOW);
        assert!(out.show_card);
    }

    #[test]
    fn a_failed_background_check_is_silent_and_stores_nothing() {
        let out = on_check_result(CheckResult::Error("offline".into()), false, "", NOW);
        assert_eq!(out.state, UpdateState::Idle);
        assert_eq!(out.store_check_time, None);
        assert!(!out.show_card);
    }

    #[test]
    fn a_failed_user_check_shows_the_error_and_stores_nothing() {
        let out = on_check_result(CheckResult::Error("offline".into()), true, "", NOW);
        assert_eq!(out.state, UpdateState::Failed("offline".into()));
        assert_eq!(out.store_check_time, None);
        assert!(out.show_card);
    }

    #[test]
    fn checking_downloading_and_installing_are_busy() {
        assert!(is_busy(&UpdateState::Checking));
        assert!(is_busy(&UpdateState::Downloading("0.4.0".into(), 0.5)));
        assert!(is_busy(&UpdateState::Installing("0.4.0".into())));
    }

    #[test]
    fn the_other_states_are_not_busy() {
        assert!(!is_busy(&UpdateState::Idle));
        assert!(!is_busy(&UpdateState::UpToDate));
        assert!(!is_busy(&UpdateState::Available("0.4.0".into())));
        assert!(!is_busy(&UpdateState::Failed("x".into())));
    }

    #[test]
    fn a_failed_install_is_not_busy() {
        // The card must accept another click after the install failed.
        assert!(!is_busy(&UpdateState::InstallFailed(
            "0.4.0".into(),
            "disk full".into()
        )));
    }
}
