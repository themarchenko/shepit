//! The pure parts of the history and the recordings store: how a row's time is written, how
//! recording files are named and pruned, and the ids a finished dictation stores. Mirrors
//! HistoryStore.swift and RecordingStore.swift; the files and the database live in the app crate.

use crate::insert::InsertOutcome;

/// How many recordings are kept (RecordingStore.swift:7).
pub const KEEP_RECORDINGS: usize = 200;

/// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's days-from-civil).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// (year, month, day, hour, minute, second, millisecond) in UTC.
fn civil(ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000,
    )
}

/// GRDB's text form of a date: `YYYY-MM-DD HH:MM:SS.SSS` in UTC.
pub fn grdb_timestamp(ms: i64) -> String {
    let (y, mo, d, h, mi, s, milli) = civil(ms);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{milli:03}")
}

/// The inverse of `grdb_timestamp`. `None` for anything else.
pub fn parse_grdb_timestamp(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 23 {
        return None;
    }
    for (i, c) in b.iter().enumerate() {
        let ok = match i {
            4 | 7 => *c == b'-',
            10 => *c == b' ',
            13 | 16 => *c == b':',
            19 => *c == b'.',
            _ => c.is_ascii_digit(),
        };
        if !ok {
            return None;
        }
    }
    let n = |a: usize, z: usize| text[a..z].parse::<i64>().ok();
    let (y, mo, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, mi, s, milli) = (n(11, 13)?, n(14, 16)?, n(17, 19)?, n(20, 23)?);
    if !(1..=12).contains(&mo) || h > 23 || mi > 59 || s > 59 {
        return None;
    }
    if d < 1 || d > days_in_month(y, mo) {
        return None;
    }
    let days = days_from_civil(y, mo, d);
    Some(((days * 24 + h) * 60 + mi) * 60_000 + s * 1000 + milli)
}

/// `yyyyMMdd-HHmmss-SSS.wav` in UTC (RecordingStore.swift:10-15 uses local time; UTC keeps the
/// sort order through a daylight-saving change).
pub fn recording_file_name(ms: i64) -> String {
    let (y, mo, d, h, mi, s, milli) = civil(ms);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}-{milli:03}.wav")
}

/// The `.wav` names to delete so that `keep` remain: the oldest in name order. Other files are
/// never named.
pub fn to_prune(names: &[String], keep: usize) -> Vec<String> {
    let mut wavs: Vec<&String> = names.iter().filter(|n| n.ends_with(".wav")).collect();
    wavs.sort();
    let excess = wavs.len().saturating_sub(keep);
    wavs.into_iter().take(excess).cloned().collect()
}

/// `InsertMethod.rawValue` for the outcomes Windows has (PasteInserter.swift:6-10).
pub fn insert_method_id(outcome: InsertOutcome) -> &'static str {
    match outcome {
        InsertOutcome::Pasted => "paste",
        InsertOutcome::ClipboardOnly => "clipboardOnly",
    }
}

/// The row status (DictationSession.swift:215).
pub fn status_id(outcome: InsertOutcome) -> &'static str {
    match outcome {
        InsertOutcome::Pasted => "inserted",
        InsertOutcome::ClipboardOnly => "clipboard",
    }
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
