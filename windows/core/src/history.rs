//! The pure parts of the history and the recordings store: how a row's time is written, how
//! recording files are named and pruned, and the ids a finished dictation stores. Mirrors
//! HistoryStore.swift and RecordingStore.swift; the files and the database live in the app crate.

use crate::insert::InsertOutcome;

/// How many recordings are kept (RecordingStore.swift:7).
pub const KEEP_RECORDINGS: usize = 200;

/// GRDB's text form of a date: `YYYY-MM-DD HH:MM:SS.SSS` in UTC.
pub fn grdb_timestamp(ms: i64) -> String {
    let _ = ms;
    todo!("history::grdb_timestamp")
}

/// The inverse of `grdb_timestamp`. `None` for anything else.
pub fn parse_grdb_timestamp(text: &str) -> Option<i64> {
    let _ = text;
    todo!("history::parse_grdb_timestamp")
}

/// `yyyyMMdd-HHmmss-SSS.wav` in UTC (RecordingStore.swift:10-15 uses local time; UTC keeps the
/// sort order through a daylight-saving change).
pub fn recording_file_name(ms: i64) -> String {
    let _ = ms;
    todo!("history::recording_file_name")
}

/// The `.wav` names to delete so that `keep` remain: the oldest in name order. Other files are
/// never named.
pub fn to_prune(names: &[String], keep: usize) -> Vec<String> {
    let _ = (names, keep);
    todo!("history::to_prune")
}

/// `InsertMethod.rawValue` for the outcomes Windows has (PasteInserter.swift:6-10).
pub fn insert_method_id(outcome: InsertOutcome) -> &'static str {
    let _ = outcome;
    todo!("history::insert_method_id")
}

/// The row status (DictationSession.swift:215).
pub fn status_id(outcome: InsertOutcome) -> &'static str {
    let _ = outcome;
    todo!("history::status_id")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOV_2023: i64 = 1_700_000_000_123;
    const LEAP_DAY: i64 = 951_782_400_000;

    #[test]
    fn the_timestamp_is_utc_text_with_milliseconds() {
        assert_eq!(grdb_timestamp(0), "1970-01-01 00:00:00.000");
        assert_eq!(grdb_timestamp(NOV_2023), "2023-11-14 22:13:20.123");
        assert_eq!(grdb_timestamp(LEAP_DAY), "2000-02-29 00:00:00.000");
    }

    #[test]
    fn the_timestamp_round_trips() {
        for ms in [0, NOV_2023, LEAP_DAY, 4_102_444_799_999] {
            assert_eq!(parse_grdb_timestamp(&grdb_timestamp(ms)), Some(ms), "{ms}");
        }
    }

    #[test]
    fn junk_is_not_a_timestamp() {
        for junk in [
            "",
            "yesterday",
            "2023-13-01 00:00:00.000",
            "2023-11-14T22:13:20.123",
            "2023-11-14 25:00:00.000",
        ] {
            assert_eq!(parse_grdb_timestamp(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn a_recording_is_named_by_its_utc_time() {
        assert_eq!(recording_file_name(NOV_2023), "20231114-221320-123.wav");
        assert_eq!(recording_file_name(0), "19700101-000000-000.wav");
    }

    #[test]
    fn recording_names_sort_in_time_order() {
        let times = [0, 999, 1_000, NOV_2023, 4_102_444_799_999];
        let mut names: Vec<String> = times.iter().map(|t| recording_file_name(*t)).collect();
        let sorted = names.clone();
        names.reverse();
        names.sort();
        assert_eq!(names, sorted);
    }

    fn wavs(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("20240101-0000{i:02}-000.wav"))
            .collect()
    }

    #[test]
    fn prune_names_the_oldest_beyond_the_keep_count() {
        let names = wavs(5);
        assert_eq!(to_prune(&names, 3), names[..2].to_vec());
    }

    #[test]
    fn prune_sorts_by_name_whatever_the_listing_order() {
        let mut names = wavs(5);
        names.reverse();
        assert_eq!(to_prune(&names, 3), wavs(5)[..2].to_vec());
    }

    #[test]
    fn prune_names_nothing_at_or_under_the_keep_count() {
        assert!(to_prune(&wavs(3), 3).is_empty());
        assert!(to_prune(&wavs(2), 3).is_empty());
        assert!(to_prune(&[], 200).is_empty());
    }

    #[test]
    fn prune_leaves_other_files_alone_and_does_not_count_them() {
        let mut names = wavs(4);
        names.push("notes.txt".into());
        names.push("00000000-000000-000.wav.bak".into());
        names.push("a.WAV".into());
        assert_eq!(to_prune(&names, 3), wavs(4)[..1].to_vec());
        assert!(to_prune(&names, 4).is_empty());
    }

    #[test]
    fn two_hundred_and_one_recordings_prune_the_oldest_one() {
        let names: Vec<String> = (0..201)
            .map(|i| recording_file_name(1_700_000_000_000 + i * 1_000))
            .collect();
        assert_eq!(to_prune(&names, KEEP_RECORDINGS), names[..1].to_vec());
        assert_eq!(KEEP_RECORDINGS, 200);
    }

    #[test]
    fn the_insert_method_and_status_ids_cover_both_outcomes() {
        assert_eq!(insert_method_id(InsertOutcome::Pasted), "paste");
        assert_eq!(
            insert_method_id(InsertOutcome::ClipboardOnly),
            "clipboardOnly"
        );
        assert_eq!(status_id(InsertOutcome::Pasted), "inserted");
        assert_eq!(status_id(InsertOutcome::ClipboardOnly), "clipboard");
    }
}
