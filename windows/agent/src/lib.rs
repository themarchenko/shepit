//! Runs the Claude Code and Codex processes behind `shepit_core::polish::pipeline::Polisher`.
//! The wire formats, timings and rotation rules live in `shepit-core`; this crate owns the
//! processes: spawning, the Job object on Windows, the reader and writer threads, the deadline.

mod claude;
mod codex;
pub mod env;
#[cfg(windows)]
mod job_win;
mod process;

pub use claude::ClaudeWorker;
pub use codex::CodexWorker;

use shepit_core::polish::lifecycle;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `CREATE_NO_WINDOW`: the release exe is GUI-subsystem, so a console child would otherwise get a
/// visible console.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The process creation flags every agent is spawned with. `CREATE_NO_WINDOW` on Windows, 0 elsewhere.
pub fn creation_flags() -> u32 {
    if cfg!(windows) {
        CREATE_NO_WINDOW
    } else {
        0
    }
}

/// Timeouts and turn limits. Injectable so tests run in seconds; `Default` is the macOS values.
#[derive(Debug, Clone, PartialEq)]
pub struct Timings {
    pub request: Duration,
    pub warm_up: Duration,
    pub max_turns: u32,
    pub standby_after_turns: u32,
    /// How long starting stays off after two failed starts in a row.
    pub pause: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Timings {
            request: Duration::from_secs_f64(lifecycle::REQUEST_TIMEOUT_SECS),
            warm_up: Duration::from_secs_f64(lifecycle::WARM_UP_TIMEOUT_SECS),
            max_turns: lifecycle::MAX_TURNS,
            standby_after_turns: lifecycle::STANDBY_AFTER_TURNS,
            pause: Duration::from_secs_f64(lifecycle::PAUSE_SECS),
        }
    }
}

/// `<base>\Shepit\claude-work`, next to the models folder.
pub fn claude_work_dir_in(base: &Path) -> PathBuf {
    base.join("Shepit").join("claude-work")
}

/// `<base>\Shepit\codex-work`.
pub fn codex_work_dir_in(base: &Path) -> PathBuf {
    base.join("Shepit").join("codex-work")
}

/// `claude_work_dir_in(%LOCALAPPDATA%)`.
pub fn default_claude_work_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| claude_work_dir_in(&d))
}

/// `codex_work_dir_in(%LOCALAPPDATA%)`.
pub fn default_codex_work_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| codex_work_dir_in(&d))
}

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    pub executable: PathBuf,
    pub model: String,
    /// `low` by default.
    pub effort: String,
    pub system_prompt: String,
    /// Holds `system-prompt.md`, `worker-debug.log` and `worker-stderr.log`; it is also the cwd.
    pub work_dir: PathBuf,
    pub timings: Timings,
    /// Added to the child's environment after the scrub. Used by tests to steer the fake agent.
    pub extra_env: Vec<(String, String)>,
}

impl ClaudeConfig {
    pub fn new(
        executable: impl Into<PathBuf>,
        model: &str,
        system_prompt: &str,
        work_dir: impl Into<PathBuf>,
    ) -> Self {
        ClaudeConfig {
            executable: executable.into(),
            model: model.to_string(),
            effort: "low".to_string(),
            system_prompt: system_prompt.to_string(),
            work_dir: work_dir.into(),
            timings: Timings::default(),
            extra_env: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodexConfig {
    pub executable: PathBuf,
    /// Empty means the model of the user's Codex config.
    pub model: String,
    /// `low` by default.
    pub effort: Option<String>,
    pub system_prompt: String,
    /// The cwd of the process and of every thread.
    pub work_dir: PathBuf,
    pub timings: Timings,
    pub extra_env: Vec<(String, String)>,
}

impl CodexConfig {
    pub fn new(
        executable: impl Into<PathBuf>,
        model: &str,
        system_prompt: &str,
        work_dir: impl Into<PathBuf>,
    ) -> Self {
        CodexConfig {
            executable: executable.into(),
            model: model.to_string(),
            effort: Some("low".to_string()),
            system_prompt: system_prompt.to_string(),
            work_dir: work_dir.into(),
            timings: Timings::default(),
            extra_env: Vec::new(),
        }
    }
}
