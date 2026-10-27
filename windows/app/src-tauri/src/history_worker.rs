//! The history thread: takes a finished dictation, saves its WAV, writes its row and tells the
//! open History window. Nothing in here may panic or block dictation: a store that failed still
//! gets the WAV, and an error is logged by kind only.

use crate::history_store::HistoryStore;
use crate::recordings;
use shepit_core::dictation::Finished;
use std::path::Path;

/// The recordings kept (RecordingStore.swift:7).
const KEEP_RECORDINGS: usize = 200;

/// Handles one finished dictation at `now_ms`. The WAV is written first (when `finished.audio`
/// is set), then the row with that file name (when `finished.record` is set and the store works).
/// `changed` is called once after a row was written, and never otherwise.
pub fn handle(
    finished: Finished,
    now_ms: i64,
    store: Option<&HistoryStore>,
    recordings: &Path,
    changed: &dyn Fn(),
) {
    // Errors are logged by kind: a path or a row would put dictated text or the user name in a log.
    let audio_file = finished.audio.as_deref().and_then(|samples| {
        let saved = recordings::save(recordings, now_ms, samples);
        if saved.is_err() {
            eprintln!("could not save the recording");
        }
        let name = saved.ok();
        if name.is_some() && recordings::prune(recordings, KEEP_RECORDINGS).is_err() {
            eprintln!("could not prune the recordings");
        }
        name
    });
    let (Some(draft), Some(store)) = (finished.record.as_ref(), store) else {
        return;
    };
    match store.save(now_ms, draft, audio_file.as_deref()) {
        Ok(_) => changed(),
        Err(err) => eprintln!("could not save the history row: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::dictation::RecordDraft;
    use shepit_core::history::recording_file_name;
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);
    const NOW: i64 = 1_700_000_000_123;
    const NAME: &str = "20231114-221320-123.wav";

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("shepit-worker-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
        fn recordings(&self) -> PathBuf {
            self.0.join("recordings")
        }
        fn wavs(&self) -> Vec<String> {
            let mut v: Vec<String> = fs::read_dir(self.recordings())
                .map(|d| {
                    d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn record() -> RecordDraft {
        RecordDraft {
            app_id: Some("code.exe".into()),
            app_name: Some("Visual Studio Code".into()),
            window_title: None,
            profile: "generic".into(),
            mode: "clean".into(),
            engine: "parakeet-v3".into(),
            raw_text: "hello".into(),
            formatted_text: "Hello.".into(),
            language: Some("en".into()),
            audio_seconds: 1.0,
            stt_ms: 350,
            llm_ms: 420,
            formatter_source: "claude".into(),
            insert_method: "paste".into(),
            status: "inserted".into(),
        }
    }

    fn finished(audio: bool, row: bool) -> Finished {
        Finished {
            id: 7,
            audio: audio.then(|| vec![0.1; 1_600]),
            record: row.then(record),
        }
    }

    /// Runs `handle` and returns how many times `changed` was called.
    fn run(f: Finished, t: &TempDir, store: Option<&HistoryStore>) -> u32 {
        let calls = Cell::new(0);
        handle(f, NOW, store, &t.recordings(), &|| {
            calls.set(calls.get() + 1)
        });
        calls.get()
    }

    #[test]
    fn a_finished_dictation_gives_a_file_and_a_row_that_names_it() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        assert_eq!(run(finished(true, true), &t, Some(&store)), 1);
        assert_eq!(t.wavs(), [NAME]);
        let rows = store.recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audio_file.as_deref(), Some(NAME));
        assert_eq!(rows[0].created_at_ms, NOW);
        assert_eq!(rows[0].raw_text, "hello");
    }

    #[test]
    fn no_audio_gives_a_row_with_a_null_file() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        assert_eq!(run(finished(false, true), &t, Some(&store)), 1);
        assert!(t.wavs().is_empty());
        assert_eq!(store.recent(10).unwrap()[0].audio_file, None);
    }

    #[test]
    fn no_record_gives_a_file_only_and_no_event() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        assert_eq!(run(finished(true, false), &t, Some(&store)), 0);
        assert_eq!(t.wavs(), [NAME]);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn nothing_to_keep_does_nothing() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        assert_eq!(run(finished(false, false), &t, Some(&store)), 0);
        assert!(t.wavs().is_empty());
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn a_missing_store_still_writes_the_wav() {
        let t = TempDir::new();
        assert_eq!(run(finished(true, true), &t, None), 0);
        assert_eq!(t.wavs(), [NAME]);
    }

    #[test]
    fn a_store_error_still_writes_the_wav_and_never_panics() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        rusqlite::Connection::open(t.0.join(crate::history_store::FILE))
            .unwrap()
            .execute_batch("DROP TABLE dictations")
            .unwrap();
        assert_eq!(run(finished(true, true), &t, Some(&store)), 0);
        assert_eq!(t.wavs(), [NAME]);
    }

    #[test]
    fn the_recordings_are_pruned_to_the_newest_two_hundred() {
        let t = TempDir::new();
        fs::create_dir_all(t.recordings()).unwrap();
        for i in 0..200 {
            let name = recording_file_name(1_600_000_000_000 + i * 1_000);
            fs::write(t.recordings().join(name), b"x").unwrap();
        }
        let store = HistoryStore::open(&t.0).unwrap();
        run(finished(true, true), &t, Some(&store));
        let wavs = t.wavs();
        assert_eq!(wavs.len(), 200);
        assert!(wavs.contains(&NAME.to_string()));
        assert!(!wavs.contains(&recording_file_name(1_600_000_000_000)));
    }
}
