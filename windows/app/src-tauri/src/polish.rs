//! Polishing in the app: a dispatcher thread and an agent thread.
//!
//! The dispatcher holds a snapshot of the settings and answers every route that needs no agent
//! (raw, fast path, secure field, no agent) at once, so those never wait behind a warm-up. Only
//! an agent route goes to the agent thread, which owns the worker. The step function is pure with
//! the clock injected, so the debounce is tested without sleeping.
//!
//! Logs carry the route, the source and milliseconds, never text.

use crate::controller::Msg;
use shepit_agent::{
    default_claude_work_dir, default_codex_work_dir, ClaudeConfig, ClaudeWorker, CodexConfig,
    CodexWorker,
};
use shepit_core::dictation::FormatJob;
use shepit_core::polish::discovery::{find_agent, Env};
use shepit_core::polish::lifecycle::{Debounce, DEBOUNCE_SECS};
use shepit_core::polish::pipeline::{build_request, format_request, Formatted, Polisher};
use shepit_core::polish::prompt::{system_prompt, FormatRequest};
use shepit_core::polish::route::{route, Route, Source};
use shepit_core::settings::{Polishing, Settings};
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Which agent to run and how. Equal specs mean the running agent stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSpec {
    /// `Claude` or `Codex`, never `Off`.
    pub provider: Polishing,
    pub executable: String,
    /// Empty for Codex: the model of the user's own Codex config.
    pub model: String,
    pub system_prompt: String,
}

/// A request that needs the agent.
#[derive(Debug, Clone)]
pub struct QueuedJob {
    pub id: u64,
    pub request: FormatRequest,
    pub fast_path_max_words: i64,
}

#[derive(Debug, Clone)]
pub enum Input {
    /// The settings at launch: builds the agent at once (the warm-up).
    Launch(Settings),
    /// New settings. The look-up of the agent is debounced; the rest applies now.
    Settings(Settings),
    Job(FormatJob),
    Tick,
}

#[derive(Debug, Clone)]
pub enum Action {
    /// A route that needed no agent.
    Answer {
        id: u64,
        text: String,
        source: Source,
        route: Route,
    },
    Queue(QueuedJob),
    /// Replace the running agent (`None` stops it).
    Replace(Option<AgentSpec>),
}

pub type Resolver = Box<dyn Fn(&Settings) -> Option<String> + Send>;

pub struct Dispatcher {
    settings: Settings,
    resolve: Resolver,
    active: Option<AgentSpec>,
    debounce: Debounce,
}

impl Dispatcher {
    pub fn new(resolve: Resolver) -> Self {
        Dispatcher {
            settings: Settings::default(),
            resolve,
            active: None,
            debounce: Debounce::new(DEBOUNCE_SECS),
        }
    }

    /// The agent the settings ask for, or `None` when polishing is off or the agent is not found.
    fn spec_for(&self, settings: &Settings) -> Option<AgentSpec> {
        let (provider, model) = match settings.polishing {
            Polishing::Off => return None,
            Polishing::Claude => (Polishing::Claude, settings.claude_model.id()),
            Polishing::Codex => (Polishing::Codex, ""),
        };
        Some(AgentSpec {
            provider,
            executable: (self.resolve)(settings)?,
            model: model.to_string(),
            system_prompt: system_prompt("Windows", Some(&settings.speaker_notes)),
        })
    }

    pub fn step(&mut self, input: Input, now: f64) -> Vec<Action> {
        match input {
            Input::Launch(settings) => {
                self.settings = settings;
                self.active = self.spec_for(&self.settings);
                self.active
                    .clone()
                    .map(|spec| Action::Replace(Some(spec)))
                    .into_iter()
                    .collect()
            }
            Input::Settings(settings) => {
                // Mode, word limits and the dictionary apply from the next job. Only a different
                // agent starts the delay, so a re-sent identical snapshot leaves the process alone.
                self.settings = settings;
                if self.spec_for(&self.settings) != self.active {
                    self.debounce.touch(now);
                }
                Vec::new()
            }
            Input::Tick => {
                if !self.debounce.due(now) {
                    return Vec::new();
                }
                let spec = self.spec_for(&self.settings);
                if spec == self.active {
                    return Vec::new();
                }
                self.active = spec.clone();
                vec![Action::Replace(spec)]
            }
            Input::Job(job) => {
                let request = build_request(&job, &self.settings);
                let fast = self.settings.fast_path_max_words;
                let which = route(&request, fast, self.active.is_some());
                if which == Route::Agent {
                    return vec![Action::Queue(QueuedJob {
                        id: job.id,
                        request,
                        fast_path_max_words: fast,
                    })];
                }
                let done = format_request(&request, fast, None, &|| false);
                vec![Action::Answer {
                    id: job.id,
                    text: done.text,
                    source: done.source,
                    route: which,
                }]
            }
        }
    }
}

/// Runs one queued job on the agent thread. `None` when the dictation is no longer wanted. The
/// request is cancelled the moment the wanted id stops being the job's (Esc, or a newer
/// dictation).
pub fn run_job(
    job: &QueuedJob,
    polisher: Option<&mut dyn Polisher>,
    wanted: &AtomicU64,
) -> Option<Formatted> {
    if wanted.load(Ordering::SeqCst) != job.id {
        return None;
    }
    let cancelled = || wanted.load(Ordering::SeqCst) != job.id;
    Some(format_request(
        &job.request,
        job.fast_path_max_words,
        polisher,
        &cancelled,
    ))
}

// Discovery against the real machine.

fn env_var(name: &str) -> Option<String> {
    std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
}

pub fn real_env() -> Env {
    Env {
        user_profile: env_var("USERPROFILE"),
        app_data: env_var("APPDATA"),
        local_app_data: env_var("LOCALAPPDATA"),
        path: env_var("PATH"),
    }
}

fn is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

/// Where Claude Code and Codex are for these settings (a configured path wins).
pub fn find_agents(settings: &Settings) -> (Option<String>, Option<String>) {
    let env = real_env();
    (
        find_agent("claude", &settings.claude_executable, &env, &is_file),
        find_agent("codex", &settings.codex_executable, &env, &is_file),
    )
}

/// The executable of the agent the settings select.
fn resolve_selected(settings: &Settings) -> Option<String> {
    let env = real_env();
    match settings.polishing {
        Polishing::Off => None,
        Polishing::Claude => find_agent("claude", &settings.claude_executable, &env, &is_file),
        Polishing::Codex => find_agent("codex", &settings.codex_executable, &env, &is_file),
    }
}

// The threads.

enum AgentCmd {
    Job(QueuedJob),
    Replace(Option<AgentSpec>),
}

/// What the rest of the app holds: settings in, format jobs in, the wanted id.
#[derive(Clone)]
pub struct PolishHandle {
    input: Sender<Input>,
    wanted: Arc<AtomicU64>,
}

impl PolishHandle {
    /// A new snapshot of the settings (a polishing or formatting change, or a re-detect).
    pub fn settings(&self, settings: Settings) {
        let _ = self.input.send(Input::Settings(settings));
    }

    /// Queues a transcript for formatting. False when the dispatcher is gone.
    pub fn format(&self, job: FormatJob) -> bool {
        self.input.send(Input::Job(job)).is_ok()
    }

    /// The id whose result is still wanted, or 0. Set before the job is sent, cleared by Esc.
    pub fn want(&self, id: u64) {
        self.wanted.store(id, Ordering::SeqCst);
    }
}

/// Starts the dispatcher and agent threads and warms the selected agent up.
pub fn start(tx: SyncSender<Msg>, settings: Settings) -> io::Result<PolishHandle> {
    let (input_tx, input_rx) = mpsc::channel::<Input>();
    let (agent_tx, agent_rx) = mpsc::channel::<AgentCmd>();
    let wanted = Arc::new(AtomicU64::new(0));
    let _ = input_tx.send(Input::Launch(settings));
    {
        let (tx, wanted) = (tx.clone(), wanted.clone());
        std::thread::Builder::new()
            .name("polish-agent".into())
            .spawn(move || agent_thread(agent_rx, tx, wanted))?;
    }
    std::thread::Builder::new()
        .name("polish-dispatch".into())
        .spawn(move || dispatch_thread(input_rx, agent_tx, tx))?;
    Ok(PolishHandle {
        input: input_tx,
        wanted,
    })
}

fn dispatch_thread(input: mpsc::Receiver<Input>, agent: Sender<AgentCmd>, tx: SyncSender<Msg>) {
    let mut dispatcher = Dispatcher::new(Box::new(resolve_selected));
    let epoch = Instant::now();
    loop {
        let msg = match input.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => Input::Tick,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let now = epoch.elapsed().as_secs_f64();
        for action in dispatcher.step(msg, now) {
            match action {
                Action::Answer {
                    id,
                    text,
                    source,
                    route,
                } => {
                    log_result(route_name(route), source, 0);
                    let _ = tx.send(Msg::Formatted { id, text });
                }
                Action::Queue(job) => {
                    if let Err(failed) = agent.send(AgentCmd::Job(job)) {
                        // The agent thread is gone: use the local text rather than hang.
                        if let AgentCmd::Job(job) = failed.0 {
                            let _ = tx.send(local_answer(&job));
                        }
                    }
                }
                Action::Replace(spec) => {
                    let _ = agent.send(AgentCmd::Replace(spec));
                }
            }
        }
    }
}

fn local_answer(job: &QueuedJob) -> Msg {
    let done = format_request(&job.request, job.fast_path_max_words, None, &|| false);
    Msg::Formatted {
        id: job.id,
        text: done.text,
    }
}

fn route_name(route: Route) -> &'static str {
    match route {
        Route::Raw => "raw",
        Route::FastPath => "fastPath",
        Route::Local => "local",
        Route::Agent => "agent",
    }
}

/// Route, source and time only: the text is the user's speech.
fn log_result(route: &str, source: Source, ms: u128) {
    eprintln!("polish: route={route} source={} ms={ms}", source.as_str());
}

enum Worker {
    Claude(ClaudeWorker),
    Codex(CodexWorker),
}

impl Worker {
    fn polisher(&mut self) -> &mut dyn Polisher {
        match self {
            Worker::Claude(w) => w,
            Worker::Codex(w) => w,
        }
    }
}

/// Builds the worker for `spec` and starts it, so the first dictation does not pay for the start.
/// A failed warm-up keeps the worker: the next request tries again, and the worker pauses itself
/// after two failures in a row.
fn build_worker(spec: &AgentSpec) -> Option<Worker> {
    let mut worker = match spec.provider {
        Polishing::Claude => {
            let dir = default_claude_work_dir()?;
            Worker::Claude(ClaudeWorker::new(ClaudeConfig::new(
                &spec.executable,
                &spec.model,
                &spec.system_prompt,
                dir,
            )))
        }
        Polishing::Codex => {
            let dir = default_codex_work_dir()?;
            Worker::Codex(CodexWorker::new(CodexConfig::new(
                &spec.executable,
                &spec.model,
                &spec.system_prompt,
                dir,
            )))
        }
        Polishing::Off => return None,
    };
    let started = Instant::now();
    let result = match &mut worker {
        Worker::Claude(w) => w.warm_up(),
        Worker::Codex(w) => w.warm_up(),
    };
    match result {
        Ok(()) => eprintln!("polish: warm-up ok ms={}", started.elapsed().as_millis()),
        Err(e) => eprintln!(
            "polish: warm-up failed ({e:?}) ms={}",
            started.elapsed().as_millis()
        ),
    }
    Some(worker)
}

/// Owns the worker. A replace drops the old one first (its process dies with it).
fn agent_thread(rx: mpsc::Receiver<AgentCmd>, tx: SyncSender<Msg>, wanted: Arc<AtomicU64>) {
    let mut worker: Option<Worker> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            AgentCmd::Replace(spec) => {
                // The old process dies before the new one starts.
                drop(worker.take());
                worker = catch_unwind(AssertUnwindSafe(|| spec.as_ref().and_then(build_worker)))
                    .unwrap_or(None);
            }
            AgentCmd::Job(job) => {
                let started = Instant::now();
                let done = catch_unwind(AssertUnwindSafe(|| {
                    run_job(&job, worker.as_mut().map(|w| w.polisher()), &wanted)
                }));
                match done {
                    // Dropped before it started: Esc already cleared the wanted id.
                    Ok(None) => log_result("agent", Source::Fallback, 0),
                    Ok(Some(done)) => {
                        log_result("agent", done.source, started.elapsed().as_millis());
                        if let Some(note) = &done.note {
                            eprintln!("polish: fallback ({note})");
                        }
                        let _ = tx.send(Msg::Formatted {
                            id: job.id,
                            text: done.text,
                        });
                    }
                    Err(_) => {
                        eprintln!("polish: agent step panicked, using local text");
                        worker = None;
                        let _ = tx.send(local_answer(&job));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::context::TargetContext;
    use shepit_core::polish::pipeline::PolishError;
    use shepit_core::profile::Mode;

    const NINE: &str = "one two three four five six seven eight nine";

    fn resolver() -> Resolver {
        Box::new(|s: &Settings| match s.polishing {
            Polishing::Claude => Some(r"C:\bin\claude.exe".to_string()),
            Polishing::Codex => Some(r"C:\bin\codex.cmd".to_string()),
            Polishing::Off => None,
        })
    }

    fn started(settings: Settings) -> Dispatcher {
        let mut d = Dispatcher::new(resolver());
        d.step(Input::Launch(settings), 0.0);
        d
    }

    fn job(id: u64, transcript: &str) -> FormatJob {
        FormatJob {
            id,
            transcript: transcript.into(),
            context: None,
        }
    }

    fn only_answer(actions: Vec<Action>) -> (u64, String, Source, Route) {
        match <[Action; 1]>::try_from(actions)
            .expect("one action")
            .into_iter()
            .next()
        {
            Some(Action::Answer {
                id,
                text,
                source,
                route,
            }) => (id, text, source, route),
            other => panic!("not an answer: {other:?}"),
        }
    }

    fn replaced(actions: &[Action]) -> Vec<Option<AgentSpec>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Replace(spec) => Some(spec.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn launch_builds_the_selected_agent_at_once() {
        let mut d = Dispatcher::new(resolver());
        let actions = d.step(Input::Launch(Settings::default()), 0.0);
        let specs = replaced(&actions);
        assert_eq!(specs.len(), 1);
        let spec = specs[0].clone().expect("a spec");
        assert_eq!(spec.provider, Polishing::Claude);
        assert_eq!(spec.executable, r"C:\bin\claude.exe");
        assert_eq!(spec.model, "haiku");
        assert!(spec.system_prompt.contains("Windows"));
    }

    #[test]
    fn launch_with_polishing_off_starts_nothing() {
        let mut d = Dispatcher::new(resolver());
        let s = Settings {
            polishing: Polishing::Off,
            ..Settings::default()
        };
        assert!(d.step(Input::Launch(s), 0.0).is_empty());
    }

    #[test]
    fn raw_mode_is_answered_at_once_without_the_agent() {
        let mut d = started(Settings {
            default_mode: Mode::Raw,
            ..Settings::default()
        });
        let (id, text, source, route) = only_answer(d.step(Input::Job(job(7, NINE)), 1.0));
        assert_eq!(
            (id, text.as_str()),
            (7, "One two three four five six seven eight nine")
        );
        assert_eq!((source, route), (Source::Passthrough, Route::Raw));
    }

    #[test]
    fn a_short_clean_dictation_takes_the_fast_path_at_once() {
        let mut d = started(Settings::default());
        let (_, _, source, route) = only_answer(d.step(Input::Job(job(1, "hello there")), 1.0));
        assert_eq!((source, route), (Source::FastPath, Route::FastPath));
    }

    #[test]
    fn a_secure_field_is_answered_locally_and_never_queued() {
        let mut d = started(Settings::default());
        let mut j = job(2, NINE);
        j.context = Some(TargetContext {
            password: Some(true),
            ..TargetContext::default()
        });
        let (_, _, source, route) = only_answer(d.step(Input::Job(j), 1.0));
        assert_eq!((source, route), (Source::Passthrough, Route::Local));
    }

    #[test]
    fn with_no_agent_found_the_text_is_answered_locally() {
        let mut d = Dispatcher::new(Box::new(|_| None));
        d.step(Input::Launch(Settings::default()), 0.0);
        let (_, _, source, route) = only_answer(d.step(Input::Job(job(3, NINE)), 1.0));
        assert_eq!((source, route), (Source::Passthrough, Route::Local));
    }

    #[test]
    fn an_agent_route_goes_to_the_queue() {
        let mut d = started(Settings::default());
        let actions = d.step(Input::Job(job(4, NINE)), 1.0);
        match actions.as_slice() {
            [Action::Queue(q)] => {
                assert_eq!(q.id, 4);
                assert_eq!(q.request.transcript, NINE);
                assert_eq!(q.fast_path_max_words, 8);
            }
            other => panic!("expected one queued job: {other:?}"),
        }
    }

    #[test]
    fn a_polishing_change_replaces_the_agent_only_after_1_5_seconds() {
        let mut d = started(Settings::default());
        let codex = Settings {
            polishing: Polishing::Codex,
            ..Settings::default()
        };
        assert!(d.step(Input::Settings(codex), 10.0).is_empty());
        assert!(d.step(Input::Tick, 11.4).is_empty());
        let specs = replaced(&d.step(Input::Tick, 11.5));
        let spec = specs[0].clone().expect("a spec");
        assert_eq!(spec.provider, Polishing::Codex);
        assert_eq!(spec.executable, r"C:\bin\codex.cmd");
        assert_eq!(spec.model, "");
        assert!(d.step(Input::Tick, 20.0).is_empty());
    }

    #[test]
    fn another_change_restarts_the_delay() {
        let mut d = started(Settings::default());
        let notes = |text: &str| Settings {
            speaker_notes: text.into(),
            ..Settings::default()
        };
        d.step(Input::Settings(notes("a")), 10.0);
        d.step(Input::Settings(notes("ab")), 11.0);
        assert!(d.step(Input::Tick, 11.6).is_empty());
        let specs = replaced(&d.step(Input::Tick, 12.5));
        assert!(specs[0].clone().unwrap().system_prompt.contains("ab"));
    }

    #[test]
    fn a_job_during_the_delay_still_goes_to_the_running_agent() {
        let mut d = started(Settings::default());
        d.step(
            Input::Settings(Settings {
                polishing: Polishing::Off,
                ..Settings::default()
            }),
            10.0,
        );
        assert!(matches!(
            d.step(Input::Job(job(5, NINE)), 10.5).as_slice(),
            [Action::Queue(_)]
        ));
    }

    #[test]
    fn switching_polishing_off_stops_the_agent_and_answers_locally() {
        let mut d = started(Settings::default());
        d.step(
            Input::Settings(Settings {
                polishing: Polishing::Off,
                ..Settings::default()
            }),
            10.0,
        );
        assert_eq!(replaced(&d.step(Input::Tick, 11.5)), [None]);
        let (_, _, source, route) = only_answer(d.step(Input::Job(job(6, NINE)), 12.0));
        assert_eq!((source, route), (Source::Passthrough, Route::Local));
    }

    #[test]
    fn a_formatting_change_applies_at_once_and_does_not_touch_the_agent() {
        let mut d = started(Settings::default());
        let raw = Settings {
            default_mode: Mode::Raw,
            ..Settings::default()
        };
        assert!(d.step(Input::Settings(raw), 10.0).is_empty());
        let (_, _, _, route) = only_answer(d.step(Input::Job(job(8, NINE)), 10.0));
        assert_eq!(route, Route::Raw);
        assert!(d.step(Input::Tick, 30.0).is_empty());
    }

    #[test]
    fn the_same_settings_again_keep_the_running_agent() {
        let mut d = started(Settings::default());
        assert!(d
            .step(Input::Settings(Settings::default()), 10.0)
            .is_empty());
        assert!(d.step(Input::Tick, 12.0).is_empty());
    }

    #[test]
    fn an_agent_installed_later_is_picked_up_when_settings_are_sent_again() {
        let found = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = found.clone();
        let mut d = Dispatcher::new(Box::new(move |_| {
            flag.load(Ordering::SeqCst)
                .then(|| r"C:\bin\claude.exe".to_string())
        }));
        assert!(d.step(Input::Launch(Settings::default()), 0.0).is_empty());
        found.store(true, Ordering::SeqCst);
        assert!(d.step(Input::Settings(Settings::default()), 5.0).is_empty());
        assert_eq!(replaced(&d.step(Input::Tick, 6.5)).len(), 1);
    }

    // The agent thread's job step.

    struct Fake {
        calls: usize,
        wanted: Option<std::sync::Arc<AtomicU64>>,
        saw_cancel: bool,
    }

    impl Polisher for Fake {
        fn source(&self) -> Source {
            Source::Claude
        }
        fn polish(&mut self, _: &str, cancelled: &dyn Fn() -> bool) -> Result<String, PolishError> {
            self.calls += 1;
            if let Some(w) = &self.wanted {
                w.store(0, Ordering::SeqCst);
            }
            if cancelled() {
                self.saw_cancel = true;
                return Err(PolishError::Cancelled);
            }
            Ok(r#"{"text":"Polished.","language":"en","new_terms":[]}"#.into())
        }
    }

    fn queued(id: u64) -> QueuedJob {
        let mut d = started(Settings::default());
        match d.step(Input::Job(job(id, NINE)), 0.0).pop() {
            Some(Action::Queue(q)) => q,
            other => panic!("not queued: {other:?}"),
        }
    }

    #[test]
    fn a_job_that_is_no_longer_wanted_is_skipped_without_calling_the_agent() {
        let mut fake = Fake {
            calls: 0,
            wanted: None,
            saw_cancel: false,
        };
        let wanted = AtomicU64::new(0);
        assert!(run_job(&queued(3), Some(&mut fake), &wanted).is_none());
        assert_eq!(fake.calls, 0);
    }

    #[test]
    fn a_wanted_job_gets_the_agent_text() {
        let mut fake = Fake {
            calls: 0,
            wanted: None,
            saw_cancel: false,
        };
        let wanted = AtomicU64::new(3);
        let out = run_job(&queued(3), Some(&mut fake), &wanted).expect("a result");
        assert_eq!(
            (out.text.as_str(), out.source),
            ("Polished.", Source::Claude)
        );
    }

    #[test]
    fn clearing_the_wanted_id_cancels_a_running_request() {
        let wanted = std::sync::Arc::new(AtomicU64::new(3));
        let mut fake = Fake {
            calls: 0,
            wanted: Some(wanted.clone()),
            saw_cancel: false,
        };
        let out = run_job(&queued(3), Some(&mut fake), &wanted);
        assert!(fake.saw_cancel);
        // The result is local text with source fallback; the controller drops it as stale.
        assert_eq!(out.map(|o| o.source), Some(Source::Fallback));
    }
}
