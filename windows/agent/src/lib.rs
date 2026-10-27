//! Runs the Claude Code and Codex processes behind `shepit_core::polish::pipeline::Polisher`.
//! The wire formats, timings and rotation rules live in `shepit-core`; this crate owns the
//! processes: spawning, the Job object on Windows, the reader and writer threads, the deadline.
//!
//! Skeleton: every body is `todo!()` until the implementation lands.
#![allow(unused_variables, clippy::diverging_sub_expression, unreachable_code)]

use shepit_core::polish::lifecycle;
use shepit_core::polish::pipeline::{PolishError, Polisher};
use shepit_core::polish::route::Source;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `CREATE_NO_WINDOW`: the release exe is GUI-subsystem, so a console child would otherwise get a
/// visible console.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The process creation flags every agent is spawned with. `CREATE_NO_WINDOW` on Windows, 0 elsewhere.
pub fn creation_flags() -> u32 {
    todo!()
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
    todo!()
}

/// `<base>\Shepit\codex-work`.
pub fn codex_work_dir_in(base: &Path) -> PathBuf {
    todo!()
}

/// `claude_work_dir_in(%LOCALAPPDATA%)`.
pub fn default_claude_work_dir() -> Option<PathBuf> {
    todo!()
}

/// `codex_work_dir_in(%LOCALAPPDATA%)`.
pub fn default_codex_work_dir() -> Option<PathBuf> {
    todo!()
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

/// One persistent `claude -p --input-format stream-json` process at a time, with a standby that
/// takes over at the turn limit. One request in flight.
pub struct ClaudeWorker {
    _config: ClaudeConfig,
}

impl ClaudeWorker {
    pub fn new(config: ClaudeConfig) -> Self {
        ClaudeWorker { _config: config }
    }

    /// Starts the process and sends the warm-up turn. A failure counts toward the pause.
    pub fn warm_up(&mut self) -> Result<(), PolishError> {
        todo!()
    }

    /// Closes stdin and kills the whole process tree of the active and standby processes.
    pub fn shutdown(&mut self) {
        todo!()
    }
}

impl Polisher for ClaudeWorker {
    fn source(&self) -> Source {
        Source::Claude
    }

    fn polish(
        &mut self,
        message: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, PolishError> {
        todo!()
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

/// One `codex app-server` process; threads rotate on it. One request in flight.
pub struct CodexWorker {
    _config: CodexConfig,
}

impl CodexWorker {
    pub fn new(config: CodexConfig) -> Self {
        CodexWorker { _config: config }
    }

    /// Starts the process, the handshake, a thread and the warm-up turn.
    pub fn warm_up(&mut self) -> Result<(), PolishError> {
        todo!()
    }

    pub fn shutdown(&mut self) {
        todo!()
    }
}

impl Polisher for CodexWorker {
    fn source(&self) -> Source {
        Source::Codex
    }

    fn polish(
        &mut self,
        message: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, PolishError> {
        todo!()
    }
}
