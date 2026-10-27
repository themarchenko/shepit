//! Helpers shared by the integration tests. Every test gets its own temp folder and steers the
//! fake agent through `extra_env`, never through this process's environment: tests run in parallel.
#![allow(dead_code)]

use serde_json::Value;
use shepit_agent::{ClaudeConfig, CodexConfig, Timings};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const FAKE: &str = env!("CARGO_BIN_EXE_shepit-fake-agent");

/// Short deadlines so a timeout test takes about a second. The warm-up limit stays generous: it
/// covers a cold process start on a busy CI machine.
pub fn timings() -> Timings {
    Timings {
        request: Duration::from_secs(1),
        warm_up: Duration::from_secs(10),
        ..Timings::default()
    }
}

pub struct Sandbox {
    pub root: tempfile::TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        Sandbox {
            root: tempfile::tempdir().unwrap(),
        }
    }
    /// The cwd of the agent process.
    pub fn work(&self) -> PathBuf {
        let p = self.root.path().join("work");
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    /// Where the fake writes `argv.json` and friends.
    pub fn out(&self) -> PathBuf {
        let p = self.root.path().join("out");
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    pub fn env(&self, extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut env = vec![(
            "SHEPIT_FAKE_DIR".to_string(),
            self.out().to_string_lossy().into_owned(),
        )];
        env.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        env
    }
    pub fn claude(&self, extra: &[(&str, &str)]) -> ClaudeConfig {
        self.claude_with(Path::new(FAKE), extra)
    }
    pub fn claude_with(&self, exe: &Path, extra: &[(&str, &str)]) -> ClaudeConfig {
        let mut c = ClaudeConfig::new(exe, "haiku", "SYSTEM PROMPT", self.work());
        c.timings = timings();
        c.extra_env = self.env(extra);
        c
    }
    pub fn codex(&self, extra: &[(&str, &str)]) -> CodexConfig {
        self.codex_with(Path::new(FAKE), extra)
    }
    pub fn codex_with(&self, exe: &Path, extra: &[(&str, &str)]) -> CodexConfig {
        let mut c = CodexConfig::new(exe, "gpt-6-luna", "SYSTEM PROMPT", self.work());
        c.timings = timings();
        c.extra_env = self.env(extra);
        c
    }
    pub fn json(&self, name: &str) -> Value {
        let bytes = std::fs::read(self.out().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        serde_json::from_slice(&bytes).unwrap()
    }
    pub fn starts(&self) -> usize {
        std::fs::read_to_string(self.out().join("starts.log"))
            .map(|s| s.lines().count())
            .unwrap_or(0)
    }
}

/// The message a dictation sends: the fake reads the text after `Transcript:\n`.
pub fn message(transcript: &str) -> String {
    format!("Transcript:\n{transcript}")
}

/// The `text` field of the JSON reply the fake produced.
pub fn reply_text(raw: &str) -> String {
    let v: Value = serde_json::from_str(raw).unwrap_or_else(|e| panic!("not JSON ({e}): {raw}"));
    v["text"].as_str().unwrap().to_string()
}

/// `[1234] hello` gives `1234`; `[thr_1234_1] hello` gives `thr_1234_1`.
pub fn tag_of(text: &str) -> String {
    text.strip_prefix('[')
        .and_then(|t| t.split_once(']'))
        .map(|(tag, _)| tag.to_string())
        .unwrap_or_else(|| panic!("no tag in {text:?}"))
}

pub fn no_cancel() -> bool {
    false
}

/// Polls until `cond` holds or `limit` passes; returns whether it held.
pub fn wait_until(limit: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}
