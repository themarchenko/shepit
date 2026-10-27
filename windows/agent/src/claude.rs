//! The Claude Code worker: one persistent `claude -p --input-format stream-json` process, a standby
//! that is warmed in the background and takes over at the turn limit (ClaudeCodeWorker.swift).

use crate::env::child_env;
use crate::process::{wait_for, AgentProcess, SpawnSpec};
use crate::{ClaudeConfig, Timings};
use serde_json::Value;
use shepit_core::polish::claude_wire::{args, decode_result, user_line, WARM_UP_MESSAGE};
use shepit_core::polish::lifecycle::FAILURES_BEFORE_PAUSE;
use shepit_core::polish::pipeline::{PolishError, Polisher};
use shepit_core::polish::route::Source;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Instant;

enum Event {
    Line(Value),
    Eof,
}

struct Live {
    proc: AgentProcess,
    rx: Receiver<Event>,
    turns: u32,
}

impl Live {
    fn alive(&mut self) -> bool {
        self.proc.is_alive()
    }
}

enum SlotState {
    Absent,
    Starting,
    Ready(Live),
}

struct Standby {
    state: SlotState,
    /// Bumped by `shutdown`; a standby that finishes under an older number is discarded.
    generation: u64,
}

fn spawn_live(config: &ClaudeConfig) -> Result<Live, PolishError> {
    let work = &config.work_dir;
    std::fs::create_dir_all(work).map_err(|_| PolishError::NotRunning)?;
    let prompt = work.join("system-prompt.md");
    // The standby starts while the active process runs: leave the file alone when it is current.
    if std::fs::read(&prompt).ok().as_deref() != Some(config.system_prompt.as_bytes()) {
        std::fs::write(&prompt, config.system_prompt.as_bytes())
            .map_err(|_| PolishError::NotRunning)?;
    }
    // Claude Code appends to its debug file; keep only the latest worker's log.
    let debug = work.join("worker-debug.log");
    let _ = std::fs::remove_file(&debug);
    let argv = args(
        &config.model,
        &config.effort,
        &prompt.to_string_lossy(),
        &debug.to_string_lossy(),
    );
    let stderr_log = work.join("worker-stderr.log");
    let spec = SpawnSpec {
        executable: &config.executable,
        args: &argv,
        cwd: work,
        env: child_env(std::env::vars(), &config.executable, &config.extra_env),
        stderr_log: &stderr_log,
        stderr_label: "worker",
    };
    let (tx, rx) = channel();
    let eof_tx = tx.clone();
    let proc = AgentProcess::spawn(
        &spec,
        move |v| {
            let _ = tx.send(Event::Line(v));
        },
        move || {
            let _ = eof_tx.send(Event::Eof);
        },
    )
    .map_err(|_| PolishError::NotRunning)?;
    Ok(Live { proc, rx, turns: 0 })
}

/// Sends one user turn and waits for its `result` event. Any failure leaves the process in an
/// unknown state; the caller drops it.
fn exchange(
    live: &mut Live,
    content: &str,
    timeout: std::time::Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<String, PolishError> {
    if !live.proc.send(user_line(content)) {
        return Err(PolishError::Exited(live.proc.exit_code()));
    }
    let result = wait_for(
        &live.rx,
        Instant::now() + timeout,
        cancelled,
        |event| match event {
            Event::Line(v) => decode_result(&v).map(|r| r.into_text()),
            Event::Eof => Some(Err(PolishError::Exited(-1))),
        },
    );
    match result {
        Err(PolishError::Exited(_)) => Err(PolishError::Exited(live.proc.exit_code())),
        Ok(text) => {
            live.turns += 1;
            Ok(text)
        }
        other => other,
    }
}

fn warm(live: &mut Live, timings: &Timings) -> Result<(), PolishError> {
    exchange(live, WARM_UP_MESSAGE, timings.warm_up, &|| false).map(|_| ())
}

pub struct ClaudeWorker {
    config: ClaudeConfig,
    active: Option<Live>,
    standby: Arc<Mutex<Standby>>,
    failures: u32,
    paused_until: Option<Instant>,
}

impl ClaudeWorker {
    pub fn new(config: ClaudeConfig) -> Self {
        ClaudeWorker {
            config,
            active: None,
            standby: Arc::new(Mutex::new(Standby {
                state: SlotState::Absent,
                generation: 0,
            })),
            failures: 0,
            paused_until: None,
        }
    }

    /// Starts the process and sends the warm-up turn. A failure counts toward the pause.
    pub fn warm_up(&mut self) -> Result<(), PolishError> {
        self.ensure_active()
    }

    /// Closes stdin and kills the whole process tree of the active and standby processes.
    pub fn shutdown(&mut self) {
        self.active = None;
        let old = {
            let mut slot = self.standby.lock().unwrap();
            slot.generation += 1;
            std::mem::replace(&mut slot.state, SlotState::Absent)
        };
        drop(old);
    }

    fn take_ready_standby(&self) -> Option<Live> {
        let mut slot = self.standby.lock().unwrap();
        match std::mem::replace(&mut slot.state, SlotState::Absent) {
            SlotState::Ready(live) => Some(live),
            SlotState::Starting => {
                slot.state = SlotState::Starting;
                None
            }
            SlotState::Absent => None,
        }
    }

    fn ensure_active(&mut self) -> Result<(), PolishError> {
        if let Some(active) = &mut self.active {
            if active.alive() {
                return Ok(());
            }
            self.active = None;
        }
        if let Some(mut standby) = self.take_ready_standby() {
            if standby.alive() {
                self.active = Some(standby);
                return Ok(());
            }
        }
        if let Some(until) = self.paused_until {
            if Instant::now() < until {
                return Err(PolishError::Paused);
            }
        }
        let started = spawn_live(&self.config).and_then(|mut live| {
            warm(&mut live, &self.config.timings)?;
            Ok(live)
        });
        match started {
            Ok(live) => {
                self.failures = 0;
                self.paused_until = None;
                self.active = Some(live);
                Ok(())
            }
            Err(e) => {
                self.failures += 1;
                if self.failures >= FAILURES_BEFORE_PAUSE {
                    self.paused_until = Some(Instant::now() + self.config.timings.pause);
                }
                Err(e)
            }
        }
    }

    fn rotate_if_needed(&mut self) {
        let Some(active) = &self.active else { return };
        let turns = active.turns;
        let timings = &self.config.timings;
        let mut slot = self.standby.lock().unwrap();
        if turns >= timings.standby_after_turns && matches!(slot.state, SlotState::Absent) {
            slot.state = SlotState::Starting;
            let generation = slot.generation;
            let config = self.config.clone();
            let shared = self.standby.clone();
            let spawned = std::thread::Builder::new()
                .name("shepit.claude.standby".into())
                .spawn(move || prepare_standby(config, shared, generation));
            if spawned.is_err() {
                slot.state = SlotState::Absent;
            }
        }
        if turns >= timings.max_turns {
            if let SlotState::Ready(_) = slot.state {
                let SlotState::Ready(next) = std::mem::replace(&mut slot.state, SlotState::Absent)
                else {
                    return;
                };
                drop(slot);
                // Replacing the active value drops the old process, which kills it.
                self.active = Some(next);
            }
        }
    }
}

fn prepare_standby(config: ClaudeConfig, shared: Arc<Mutex<Standby>>, generation: u64) {
    let result = spawn_live(&config).and_then(|mut live| {
        warm(&mut live, &config.timings)?;
        Ok(live)
    });
    let mut slot = shared.lock().unwrap();
    let current = slot.generation == generation;
    match result {
        Ok(live) if current => slot.state = SlotState::Ready(live),
        Err(_) if current => slot.state = SlotState::Absent,
        // A shutdown happened meanwhile: `live` drops here and the process dies.
        _ => {}
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
        self.ensure_active()?;
        let timeout = self.config.timings.request;
        let live = self.active.as_mut().ok_or(PolishError::NotRunning)?;
        match exchange(live, message, timeout, cancelled) {
            Ok(text) => {
                self.rotate_if_needed();
                Ok(text)
            }
            Err(e) => {
                // Timeout, exit, cancel or an error result: the process state is unknown.
                self.active = None;
                Err(e)
            }
        }
    }
}

impl Drop for ClaudeWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}
