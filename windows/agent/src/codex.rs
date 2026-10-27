//! The Codex worker: one `codex app-server` process, JSON-RPC over stdio, threads rotating on it
//! (CodexFormatter.swift). Requests are matched by id and turns are routed by thread id, because
//! the standby thread warms up while a dictation runs on the active one.

use crate::env::child_env;
use crate::process::{wait_for, AgentProcess, SpawnSpec, WriterMsg};
use crate::CodexConfig;
use serde_json::{json, Map, Value};
use shepit_core::polish::codex_wire::{
    arguments, classify, encode_line, initialize_params, reply_to_request,
    thread_id_from_start_result, thread_start_params, turn_id_from_start_result, turn_start_params,
    Incoming, Outcome, TurnCollector, WARM_UP_MESSAGE,
};
use shepit_core::polish::lifecycle::FAILURES_BEFORE_PAUSE;
use shepit_core::polish::pipeline::{PolishError, Polisher};
use shepit_core::polish::route::Source;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

enum Fail {
    Exited,
    Rpc(String),
}

impl Fail {
    fn into_error(self) -> PolishError {
        match self {
            Fail::Exited => PolishError::Exited(-1),
            Fail::Rpc(text) => PolishError::Api(text),
        }
    }
}

type Response = Result<Map<String, Value>, Fail>;
type TurnMessage = Result<Outcome, Fail>;

struct ActiveTurn {
    collector: TurnCollector,
    tx: Sender<TurnMessage>,
}

#[derive(Default)]
struct Shared {
    next_id: i64,
    pending: HashMap<i64, Sender<Response>>,
    turns: HashMap<String, ActiveTurn>,
    exited: bool,
}

/// The JSON-RPC connection. Written to from any thread, read by the process's reader thread.
struct Conn {
    shared: Mutex<Shared>,
    writer: OnceLock<Sender<WriterMsg>>,
}

impl Conn {
    fn write(&self, message: &Value) -> bool {
        match self.writer.get() {
            Some(w) => w
                .send(WriterMsg::Data(encode_line(message).into_bytes()))
                .is_ok(),
            None => false,
        }
    }

    fn exited(&self) -> bool {
        self.shared.lock().unwrap().exited
    }

    fn request(&self, method: &str, params: Value) -> Result<Receiver<Response>, PolishError> {
        let (tx, rx) = channel();
        let id = {
            let mut s = self.shared.lock().unwrap();
            if s.exited {
                return Err(PolishError::NotRunning);
            }
            s.next_id += 1;
            let id = s.next_id;
            s.pending.insert(id, tx);
            id
        };
        if !self.write(&json!({"method": method, "id": id, "params": params})) {
            self.shared.lock().unwrap().pending.remove(&id);
            return Err(PolishError::Exited(-1));
        }
        Ok(rx)
    }

    fn call(
        &self,
        method: &str,
        params: Value,
        deadline: Instant,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Map<String, Value>, PolishError> {
        let rx = self.request(method, params)?;
        wait_for(&rx, deadline, cancelled, |m| {
            Some(m.map_err(Fail::into_error))
        })
    }

    fn on_line(&self, message: Value) {
        match classify(&message) {
            None => {}
            Some(Incoming::Response { id, result, error }) => {
                let tx = self.shared.lock().unwrap().pending.remove(&id);
                if let Some(tx) = tx {
                    let _ = tx.send(match error {
                        Some(text) => Err(Fail::Rpc(text)),
                        None => Ok(result),
                    });
                }
            }
            // With approvals set to never none should arrive; answer so a turn never waits on us.
            Some(Incoming::Request { id, method, .. }) => {
                self.write(&reply_to_request(&method, &id));
            }
            Some(Incoming::Notification { method, params }) => {
                let Some(thread) = params.get("threadId").and_then(Value::as_str) else {
                    return;
                };
                let mut s = self.shared.lock().unwrap();
                if let Some(turn) = s.turns.get_mut(thread) {
                    if let Some(outcome) = turn.collector.consume(&method, &params) {
                        let _ = turn.tx.send(Ok(outcome));
                    }
                }
            }
        }
    }

    fn on_eof(&self) {
        let mut s = self.shared.lock().unwrap();
        s.exited = true;
        for (_, tx) in s.pending.drain() {
            let _ = tx.send(Err(Fail::Exited));
        }
        for turn in s.turns.values() {
            let _ = turn.tx.send(Err(Fail::Exited));
        }
    }

    /// Sends `turn/start` on a thread and waits for `turn/completed`. The inner error is the text
    /// of a turn Codex itself reported as failed; the process is healthy then.
    fn run_turn(
        &self,
        thread_id: &str,
        text: &str,
        effort: Option<&str>,
        deadline: Instant,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Result<String, String>, PolishError> {
        let (tx, rx) = channel();
        {
            let mut s = self.shared.lock().unwrap();
            if s.exited {
                return Err(PolishError::NotRunning);
            }
            s.turns.insert(
                thread_id.to_string(),
                ActiveTurn {
                    collector: TurnCollector::new(None),
                    tx,
                },
            );
        }
        struct Registered<'a>(&'a Conn, &'a str);
        impl Drop for Registered<'_> {
            fn drop(&mut self) {
                self.0.shared.lock().unwrap().turns.remove(self.1);
            }
        }
        let _registered = Registered(self, thread_id);

        let started = self.call(
            "turn/start",
            turn_start_params(thread_id, text, effort),
            deadline,
            cancelled,
        )?;
        if let Some(id) = turn_id_from_start_result(&started) {
            let mut s = self.shared.lock().unwrap();
            if let Some(turn) = s.turns.get_mut(thread_id) {
                if turn.collector.turn_id.is_none() {
                    turn.collector.turn_id = Some(id);
                }
            }
        }
        wait_for(&rx, deadline, cancelled, |m| match m {
            Ok(Outcome::Reply(text)) => Some(Ok(Ok(text))),
            Ok(Outcome::Failed(reason)) => Some(Ok(Err(reason))),
            Err(fail) => Some(Err(fail.into_error())),
        })
    }

    /// Starts a thread and runs the warm-up turn, which opens the model connection and fills the
    /// prompt cache.
    fn start_warm_thread(
        &self,
        config: &CodexConfig,
        deadline: Instant,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, PolishError> {
        let work = config.work_dir.to_string_lossy();
        let result = self.call(
            "thread/start",
            thread_start_params(&config.model, &config.system_prompt, &work),
            deadline,
            cancelled,
        )?;
        let thread_id = thread_id_from_start_result(&result)
            .ok_or_else(|| PolishError::Api("thread/start returned no thread id".into()))?;
        let effort = config.effort.as_deref();
        match self.run_turn(&thread_id, WARM_UP_MESSAGE, effort, deadline, cancelled)? {
            Ok(_) => Ok(thread_id),
            Err(reason) => Err(PolishError::Api(reason)),
        }
    }

    /// Lets Codex unload a thread we no longer use. The answer is not awaited.
    fn release(&self, thread_id: &str) {
        let _ = self.request("thread/unsubscribe", json!({"threadId": thread_id}));
    }
}

struct Server {
    proc: AgentProcess,
    conn: Arc<Conn>,
}

impl Server {
    fn alive(&mut self) -> bool {
        self.proc.is_alive() && !self.conn.exited()
    }
}

fn spawn_server(config: &CodexConfig) -> Result<Server, PolishError> {
    std::fs::create_dir_all(&config.work_dir).map_err(|_| PolishError::NotRunning)?;
    let argv = arguments();
    let stderr_log = config.work_dir.join("codex-stderr.log");
    let spec = SpawnSpec {
        executable: &config.executable,
        args: &argv,
        cwd: &config.work_dir,
        env: child_env(std::env::vars(), &config.executable, &config.extra_env),
        stderr_log: &stderr_log,
        stderr_label: "codex app-server",
    };
    let conn = Arc::new(Conn {
        shared: Mutex::new(Shared::default()),
        writer: OnceLock::new(),
    });
    let (on_line, on_eof) = (conn.clone(), conn.clone());
    let proc = AgentProcess::spawn(&spec, move |v| on_line.on_line(v), move || on_eof.on_eof())
        .map_err(|_| PolishError::NotRunning)?;
    let _ = conn.writer.set(proc.writer());
    Ok(Server { proc, conn })
}

fn handshake(
    conn: &Conn,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PolishError> {
    let params = initialize_params(env!("CARGO_PKG_VERSION"));
    conn.call("initialize", params, deadline, cancelled)?;
    if conn.write(&json!({"method": "initialized", "params": {}})) {
        Ok(())
    } else {
        Err(PolishError::Exited(-1))
    }
}

enum SlotState {
    Absent,
    Starting,
    Ready(String),
}

struct Standby {
    state: SlotState,
    /// Bumped whenever the server is dropped; a standby thread that finishes under an older
    /// number belongs to a dead process and is forgotten.
    generation: u64,
}

pub struct CodexWorker {
    config: CodexConfig,
    server: Option<Server>,
    active_thread: Option<String>,
    active_turns: u32,
    standby: Arc<Mutex<Standby>>,
    failures: u32,
    paused_until: Option<Instant>,
}

impl CodexWorker {
    pub fn new(config: CodexConfig) -> Self {
        CodexWorker {
            config,
            server: None,
            active_thread: None,
            active_turns: 0,
            standby: Arc::new(Mutex::new(Standby {
                state: SlotState::Absent,
                generation: 0,
            })),
            failures: 0,
            paused_until: None,
        }
    }

    /// Starts the process, the handshake, a thread and the warm-up turn.
    pub fn warm_up(&mut self) -> Result<(), PolishError> {
        self.ensure_thread(&|| false).map(|_| ())
    }

    pub fn shutdown(&mut self) {
        self.drop_server();
    }

    fn drop_server(&mut self) {
        {
            let mut slot = self.standby.lock().unwrap();
            slot.generation += 1;
            slot.state = SlotState::Absent;
        }
        // Dropping the server closes stdin, kills the process and its job.
        self.server = None;
        self.active_thread = None;
        self.active_turns = 0;
    }

    fn promote_standby(&mut self) -> Option<String> {
        let next = {
            let mut slot = self.standby.lock().unwrap();
            match std::mem::replace(&mut slot.state, SlotState::Absent) {
                SlotState::Ready(id) => id,
                SlotState::Starting => {
                    slot.state = SlotState::Starting;
                    return None;
                }
                SlotState::Absent => return None,
            }
        };
        if let (Some(server), Some(old)) = (&self.server, self.active_thread.take()) {
            server.conn.release(&old);
        }
        self.active_thread = Some(next.clone());
        self.active_turns = 1;
        Some(next)
    }

    /// `cancelled` reaches the warm-up of a respawned server, so Esc or a newer dictation does not
    /// wait out the 40 s limit while the agent thread stays busy.
    fn ensure_thread(&mut self, cancelled: &dyn Fn() -> bool) -> Result<String, PolishError> {
        if self.server.as_mut().is_some_and(|s| s.alive()) {
            if let Some(thread) = &self.active_thread {
                return Ok(thread.clone());
            }
            if let Some(thread) = self.promote_standby() {
                return Ok(thread);
            }
        }
        if let Some(until) = self.paused_until {
            if Instant::now() < until {
                return Err(PolishError::Paused);
            }
        }
        self.start_server(cancelled)
    }

    fn start_server(&mut self, cancelled: &dyn Fn() -> bool) -> Result<String, PolishError> {
        self.drop_server();
        let deadline = Instant::now() + self.config.timings.warm_up;
        let started = spawn_server(&self.config).and_then(|server| {
            handshake(&server.conn, deadline, cancelled)?;
            let thread = server
                .conn
                .start_warm_thread(&self.config, deadline, cancelled)?;
            Ok((server, thread))
        });
        match started {
            Ok((server, thread)) => {
                self.failures = 0;
                self.paused_until = None;
                self.server = Some(server);
                self.active_thread = Some(thread.clone());
                self.active_turns = 1;
                Ok(thread)
            }
            // The user gave up, the server did not fail: no strike toward the pause.
            Err(PolishError::Cancelled) => Err(PolishError::Cancelled),
            Err(e) => {
                // `started` dropped the half-built server, which killed it.
                self.failures += 1;
                if self.failures >= FAILURES_BEFORE_PAUSE {
                    self.paused_until = Some(Instant::now() + self.config.timings.pause);
                }
                Err(e)
            }
        }
    }

    fn rotate_if_needed(&mut self) {
        let Some(server) = &self.server else { return };
        let timings = &self.config.timings;
        let start_standby = {
            let mut slot = self.standby.lock().unwrap();
            if self.active_turns >= timings.standby_after_turns
                && matches!(slot.state, SlotState::Absent)
            {
                slot.state = SlotState::Starting;
                Some(slot.generation)
            } else {
                None
            }
        };
        if let Some(generation) = start_standby {
            let conn = server.conn.clone();
            let config = self.config.clone();
            let shared = self.standby.clone();
            let spawned = std::thread::Builder::new()
                .name("shepit.codex.standby".into())
                .spawn(move || prepare_standby(conn, config, shared, generation));
            if spawned.is_err() {
                self.standby.lock().unwrap().state = SlotState::Absent;
            }
        }
        if self.active_turns >= timings.max_turns {
            self.promote_standby();
        }
    }
}

fn prepare_standby(
    conn: Arc<Conn>,
    config: CodexConfig,
    shared: Arc<Mutex<Standby>>,
    generation: u64,
) {
    let deadline = Instant::now() + config.timings.warm_up;
    let stale = || shared.lock().unwrap().generation != generation;
    let result = conn.start_warm_thread(&config, deadline, &stale);
    let mut slot = shared.lock().unwrap();
    if slot.generation != generation {
        return;
    }
    slot.state = match result {
        Ok(thread) => SlotState::Ready(thread),
        Err(_) => SlotState::Absent,
    };
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
        let thread = self.ensure_thread(cancelled)?;
        let conn = self
            .server
            .as_ref()
            .map(|s| s.conn.clone())
            .ok_or(PolishError::NotRunning)?;
        let deadline = Instant::now() + self.config.timings.request;
        let effort = self.config.effort.clone();
        match conn.run_turn(&thread, message, effort.as_deref(), deadline, cancelled) {
            Ok(Ok(text)) => {
                self.active_turns += 1;
                self.rotate_if_needed();
                Ok(text)
            }
            // Codex itself reported the failure (usage limit, upstream error): the process is
            // healthy, keep it.
            Ok(Err(reason)) => Err(PolishError::Api(reason)),
            Err(e) => {
                // After a timeout, a cancel or an exit the process is in an unknown state.
                let e = match (e, self.server.as_mut()) {
                    (PolishError::Exited(_), Some(s)) => PolishError::Exited(s.proc.exit_code()),
                    (e, _) => e,
                };
                self.drop_server();
                Err(e)
            }
        }
    }
}

impl Drop for CodexWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}
