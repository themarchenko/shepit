//! The history database: one SQLite file with the `dictations` table of the macOS app
//! (HistoryStore.swift:69-95), same column names, types and order. Migrations use `PRAGMA
//! user_version` (2); nothing claims either app can open the other's file. Errors carry a kind,
//! never a path or dictated text.

use shepit_core::dictation::RecordDraft;
use std::path::Path;

/// The database file inside the local data directory.
pub const FILE: &str = "shepit.sqlite";

/// The newest rows the window loads (HistoryView.swift:52).
pub const RECENT_LIMIT: usize = 200;

/// One stored row. `created_at_ms` is Unix time in ms; the file holds it as UTC text.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryRow {
    pub id: i64,
    pub created_at_ms: i64,
    pub bundle_id: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub profile: String,
    pub mode: String,
    pub engine: String,
    pub raw_text: String,
    pub formatted_text: String,
    pub final_text: Option<String>,
    pub language: Option<String>,
    pub audio_seconds: f64,
    pub stt_ms: i64,
    pub llm_ms: i64,
    pub formatter_source: String,
    pub insert_method: String,
    pub status: String,
    pub audio_file: Option<String>,
}

pub struct HistoryStore {
    _stub: (),
}

impl HistoryStore {
    /// Opens (and creates) `dir/shepit.sqlite`, migrates it to user_version 2 and turns
    /// `secure_delete` on.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let _ = dir;
        todo!("HistoryStore::open")
    }

    /// Inserts one row stamped `created_at_ms`. `finalText` stays NULL.
    pub fn save(
        &self,
        created_at_ms: i64,
        draft: &RecordDraft,
        audio_file: Option<&str>,
    ) -> Result<i64, String> {
        let _ = (created_at_ms, draft, audio_file);
        todo!("HistoryStore::save")
    }

    /// The newest `limit` rows, newest first.
    pub fn recent(&self, limit: usize) -> Result<Vec<HistoryRow>, String> {
        let _ = limit;
        todo!("HistoryStore::recent")
    }

    pub fn count(&self) -> Result<i64, String> {
        todo!("HistoryStore::count")
    }

    /// Deletes every row, then VACUUMs so the text does not stay in free pages.
    pub fn clear(&self) -> Result<(), String> {
        todo!("HistoryStore::clear")
    }

    /// True when this connection overwrites deleted content.
    pub fn secure_delete(&self) -> bool {
        todo!("HistoryStore::secure_delete")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A fresh directory path under the system temp dir; not created. Removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("shepit-history-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join(FILE)
        }
        fn raw(&self) -> Connection {
            Connection::open(self.file()).unwrap()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const NOV_2023: i64 = 1_700_000_000_123;

    fn draft(raw_text: &str) -> RecordDraft {
        RecordDraft {
            app_id: Some("code.exe".into()),
            app_name: Some("Visual Studio Code".into()),
            window_title: Some("main.rs - shepit".into()),
            profile: "generic".into(),
            mode: "clean".into(),
            engine: "parakeet-v3".into(),
            raw_text: raw_text.into(),
            formatted_text: format!("{raw_text}."),
            language: Some("en".into()),
            audio_seconds: 2.5,
            stt_ms: 350,
            llm_ms: 420,
            formatter_source: "claude".into(),
            insert_method: "paste".into(),
            status: "inserted".into(),
        }
    }

    fn user_version(db: &Connection) -> i64 {
        db.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    /// `PRAGMA table_info`: name, declared type, not-null flag, primary-key flag.
    fn columns(db: &Connection) -> Vec<(String, String, bool, bool)> {
        let mut stmt = db.prepare("PRAGMA table_info(dictations)").unwrap();
        stmt.query_map([], |r| {
            Ok((
                r.get(1)?,
                r.get(2)?,
                r.get::<_, i64>(3)? != 0,
                r.get::<_, i64>(5)? != 0,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    }

    fn col(name: &str, ty: &str, not_null: bool) -> (String, String, bool, bool) {
        (name.into(), ty.into(), not_null, false)
    }

    // HistoryStore.swift:69-95: the v1 table, then v2 adds audioFile.
    fn macos_columns() -> Vec<(String, String, bool, bool)> {
        vec![
            ("id".into(), "INTEGER".into(), false, true),
            col("createdAt", "DATETIME", true),
            col("bundleID", "TEXT", false),
            col("appName", "TEXT", false),
            col("windowTitle", "TEXT", false),
            col("profile", "TEXT", true),
            col("mode", "TEXT", true),
            col("engine", "TEXT", true),
            col("rawText", "TEXT", true),
            col("formattedText", "TEXT", true),
            col("finalText", "TEXT", false),
            col("language", "TEXT", false),
            col("audioSeconds", "DOUBLE", true),
            col("sttMs", "INTEGER", true),
            col("llmMs", "INTEGER", true),
            col("formatterSource", "TEXT", true),
            col("insertMethod", "TEXT", true),
            col("status", "TEXT", true),
            col("audioFile", "TEXT", false),
        ]
    }

    #[test]
    fn opening_creates_the_directory_and_a_version_2_file() {
        let t = TempDir::new();
        HistoryStore::open(&t.0.join("nested")).unwrap();
        let db = Connection::open(t.0.join("nested").join(FILE)).unwrap();
        assert_eq!(user_version(&db), 2);
    }

    #[test]
    fn the_table_has_the_macos_names_types_and_order() {
        let t = TempDir::new();
        HistoryStore::open(&t.0).unwrap();
        assert_eq!(columns(&t.raw()), macos_columns());
    }

    #[test]
    fn a_version_1_file_gains_audio_file_and_keeps_its_rows() {
        let t = TempDir::new();
        fs::create_dir_all(&t.0).unwrap();
        {
            let db = t.raw();
            let v1: Vec<String> = macos_columns()[..18]
                .iter()
                .map(|(n, ty, nn, pk)| match (pk, nn) {
                    (true, _) => format!("{n} {ty} PRIMARY KEY AUTOINCREMENT"),
                    (_, true) => format!("{n} {ty} NOT NULL"),
                    _ => format!("{n} {ty}"),
                })
                .collect();
            db.execute_batch(&format!(
                "CREATE TABLE dictations ({});
                 INSERT INTO dictations (createdAt, profile, mode, engine, rawText, formattedText,
                   audioSeconds, sttMs, llmMs, formatterSource, insertMethod, status)
                 VALUES ('2023-11-14 22:13:20.123', 'generic', 'clean', 'parakeet-v3', 'old', 'Old.',
                   1.0, 1, 2, 'claude', 'paste', 'inserted');
                 PRAGMA user_version = 1;",
                v1.join(", ")
            ))
            .unwrap();
        }
        let store = HistoryStore::open(&t.0).unwrap();
        assert_eq!(user_version(&t.raw()), 2);
        assert_eq!(columns(&t.raw()), macos_columns());
        let rows = store.recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].raw_text.as_str(), rows[0].audio_file.clone()),
            ("old", None)
        );
    }

    #[test]
    fn reopening_a_version_2_file_keeps_its_rows() {
        let t = TempDir::new();
        HistoryStore::open(&t.0)
            .unwrap()
            .save(NOV_2023, &draft("kept"), None)
            .unwrap();
        let store = HistoryStore::open(&t.0).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(user_version(&t.raw()), 2);
    }

    #[test]
    fn a_saved_row_comes_back_with_every_value() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        let id = store
            .save(NOV_2023, &draft("hello"), Some("20231114-221320-123.wav"))
            .unwrap();
        let rows = store.recent(10).unwrap();
        assert_eq!(
            rows,
            vec![HistoryRow {
                id,
                created_at_ms: NOV_2023,
                bundle_id: Some("code.exe".into()),
                app_name: Some("Visual Studio Code".into()),
                window_title: Some("main.rs - shepit".into()),
                profile: "generic".into(),
                mode: "clean".into(),
                engine: "parakeet-v3".into(),
                raw_text: "hello".into(),
                formatted_text: "hello.".into(),
                final_text: None,
                language: Some("en".into()),
                audio_seconds: 2.5,
                stt_ms: 350,
                llm_ms: 420,
                formatter_source: "claude".into(),
                insert_method: "paste".into(),
                status: "inserted".into(),
                audio_file: Some("20231114-221320-123.wav".into()),
            }]
        );
    }

    #[test]
    fn a_row_without_audio_or_app_stores_nulls() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        let mut d = draft("x");
        d.app_id = None;
        d.app_name = None;
        d.window_title = None;
        d.language = None;
        store.save(NOV_2023, &d, None).unwrap();
        let (bundle, audio, final_text): (Option<String>, Option<String>, Option<String>) = t
            .raw()
            .query_row(
                "SELECT bundleID, audioFile, finalText FROM dictations",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((bundle, audio, final_text), (None, None, None));
    }

    #[test]
    fn created_at_is_utc_text_with_milliseconds() {
        let t = TempDir::new();
        HistoryStore::open(&t.0)
            .unwrap()
            .save(NOV_2023, &draft("x"), None)
            .unwrap();
        let (text, kind): (String, String) = t
            .raw()
            .query_row(
                "SELECT createdAt, typeof(createdAt) FROM dictations",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (text.as_str(), kind.as_str()),
            ("2023-11-14 22:13:20.123", "text")
        );
    }

    #[test]
    fn recent_is_newest_first_whatever_the_save_order() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        for (ms, text) in [(3_000, "third"), (1_000, "first"), (2_000, "second")] {
            store.save(ms, &draft(text), None).unwrap();
        }
        let got: Vec<String> = store
            .recent(10)
            .unwrap()
            .into_iter()
            .map(|r| r.raw_text)
            .collect();
        assert_eq!(got, ["third", "second", "first"]);
    }

    #[test]
    fn recent_stops_at_the_limit_and_count_sees_every_row() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        for i in 0..205 {
            store
                .save(1_000 + i, &draft(&format!("row {i}")), None)
                .unwrap();
        }
        let rows = store.recent(RECENT_LIMIT).unwrap();
        assert_eq!(rows.len(), 200);
        assert_eq!(rows[0].raw_text, "row 204");
        assert_eq!(rows[199].raw_text, "row 5");
        assert_eq!(store.count().unwrap(), 205);
        assert_eq!(RECENT_LIMIT, 200);
    }

    #[test]
    fn an_empty_store_has_no_rows() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        assert_eq!(
            (store.recent(200).unwrap().len(), store.count().unwrap()),
            (0, 0)
        );
    }

    #[test]
    fn secure_delete_is_on() {
        let t = TempDir::new();
        assert!(HistoryStore::open(&t.0).unwrap().secure_delete());
    }

    #[test]
    fn clear_deletes_every_row() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        for i in 0..5 {
            store.save(1_000 + i, &draft("x"), None).unwrap();
        }
        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), 0);
        assert!(store.recent(200).unwrap().is_empty());
        store.save(9_000, &draft("after"), None).unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn clear_leaves_no_trace_of_the_text_in_the_file() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        let marker = "ZZ-private-dictation-marker-ZZ";
        store.save(NOV_2023, &draft(marker), None).unwrap();
        let has = |bytes: &[u8]| bytes.windows(marker.len()).any(|w| w == marker.as_bytes());
        assert!(
            has(&fs::read(t.file()).unwrap()),
            "the control: the text is in the file"
        );
        store.clear().unwrap();
        assert!(!has(&fs::read(t.file()).unwrap()));
    }

    #[test]
    fn a_file_that_is_not_sqlite_gives_an_error_and_is_left_alone() {
        let t = TempDir::new();
        fs::create_dir_all(&t.0).unwrap();
        let junk = b"this is not a database, it is long enough to look like one ".repeat(40);
        fs::write(t.file(), &junk).unwrap();
        assert!(HistoryStore::open(&t.0).is_err());
        assert_eq!(fs::read(t.file()).unwrap(), junk);
    }
}
