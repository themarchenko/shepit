//! The Codex worker against the fake app-server: ports of CodexFormatterProcessTests
//! (CodexFormatterTests.swift:319-400).
mod common;

use common::*;
use serde_json::json;
use shepit_agent::CodexWorker;
use shepit_core::polish::codex_wire;
use shepit_core::polish::pipeline::{PolishError, Polisher};
use std::time::{Duration, Instant};

fn ask(w: &mut CodexWorker, transcript: &str) -> Result<String, PolishError> {
    w.polish(&message(transcript), &no_cancel)
}

fn say(w: &mut CodexWorker, transcript: &str) -> String {
    reply_text(&ask(w, transcript).unwrap())
}

/// `thr_<pid>_<n>` of a reply.
fn thread(text: &str) -> String {
    tag_of(text)
}

/// The `<pid>` of `thr_<pid>_<n>`.
fn process(thread: &str) -> String {
    thread.split('_').nth(1).unwrap().to_string()
}

#[test]
fn formats_through_the_app_server() {
    let sb = Sandbox::new();
    let mut w = CodexWorker::new(sb.codex(&[]));
    w.warm_up().unwrap();
    let text = say(&mut w, "hello there");
    assert!(text.ends_with("] hello there"), "{text}");
    assert_eq!(sb.json("argv.json"), json!(codex_wire::arguments()));
    let start = sb.json("thread-start.json");
    assert_eq!(start["baseInstructions"], "SYSTEM PROMPT");
    assert_eq!(start["ephemeral"], true);
    assert_eq!(start["cwd"], sb.work().to_string_lossy().as_ref());
    assert_eq!(start["model"], "gpt-6-luna");
    w.shutdown();
}

#[test]
fn an_empty_model_leaves_the_model_to_the_codex_config() {
    let sb = Sandbox::new();
    let mut cfg = sb.codex(&[]);
    cfg.model = String::new();
    let mut w = CodexWorker::new(cfg);
    say(&mut w, "x");
    assert!(sb.json("thread-start.json").get("model").is_none());
    w.shutdown();
}

#[test]
fn cyrillic_survives_stdin_and_stdout() {
    let sb = Sandbox::new();
    let mut w = CodexWorker::new(sb.codex(&[]));
    let text = say(&mut w, "Привіт, світе! ґєї");
    assert!(text.ends_with("] Привіт, світе! ґєї"), "{text}");
    w.shutdown();
}

#[test]
fn an_approval_request_is_declined() {
    let sb = Sandbox::new();
    let mut w = CodexWorker::new(sb.codex(&[]));
    assert_eq!(say(&mut w, "approve"), "decline");
    w.shutdown();
}

#[test]
fn a_failed_turn_keeps_the_process_and_the_thread() {
    let sb = Sandbox::new();
    let mut w = CodexWorker::new(sb.codex(&[]));
    let first = thread(&say(&mut w, "one"));
    assert_eq!(
        ask(&mut w, "fail"),
        Err(PolishError::Api(
            "You've hit your usage limit. (usageLimitExceeded)".into()
        ))
    );
    let after = thread(&say(&mut w, "two"));
    assert_eq!(after, first);
    assert_eq!(sb.starts(), 1);
    w.shutdown();
}

#[test]
fn a_timeout_restarts_the_process() {
    let sb = Sandbox::new();
    let mut w = CodexWorker::new(sb.codex(&[])); // request timeout: 1 s
    let first = thread(&say(&mut w, "one"));
    let t0 = Instant::now();
    assert_eq!(ask(&mut w, "hang"), Err(PolishError::Timeout));
    let took = t0.elapsed();
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_secs(5),
        "{took:?}"
    );
    let after = thread(&say(&mut w, "two"));
    assert_ne!(
        after, first,
        "a new process should serve the next dictation"
    );
    assert_ne!(process(&after), process(&first));
    w.shutdown();
}

#[test]
fn an_exit_is_reported_and_recovered() {
    let sb = Sandbox::new();
    let mut w = CodexWorker::new(sb.codex(&[]));
    say(&mut w, "one");
    assert!(matches!(ask(&mut w, "exit"), Err(PolishError::Exited(_))));
    assert!(say(&mut w, "two").ends_with("] two"));
    w.shutdown();
}

#[test]
fn the_thread_rotates_to_a_standby_thread_on_the_same_process() {
    let sb = Sandbox::new();
    let mut cfg = sb.codex(&[]);
    cfg.timings.standby_after_turns = 2;
    cfg.timings.max_turns = 3;
    let mut w = CodexWorker::new(cfg);
    // The warm-up is turn 1, as on macOS.
    let first = thread(&say(&mut w, "a")); // turn 2: the standby starts
    std::thread::sleep(Duration::from_millis(500));
    let second = thread(&say(&mut w, "b")); // turn 3: the standby takes over after the reply
    let third = thread(&say(&mut w, "c"));
    assert_eq!(second, first);
    assert_ne!(third, first);
    assert_eq!(process(&third), process(&first), "same process, new thread");
    assert_eq!(sb.starts(), 1);
    w.shutdown();
}

#[test]
fn two_failed_starts_in_a_row_pause_starting() {
    let sb = Sandbox::new();
    let mut w = CodexWorker::new(sb.codex(&[("SHEPIT_FAKE_START", "die")]));
    assert!(ask(&mut w, "one").is_err());
    assert!(ask(&mut w, "two").is_err());
    assert_eq!(ask(&mut w, "three"), Err(PolishError::Paused));
    assert_eq!(sb.starts(), 2);
}

#[test]
fn a_cancel_drops_the_process() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let sb = Sandbox::new();
    let mut cfg = sb.codex(&[]);
    cfg.timings.request = Duration::from_secs(20);
    let mut w = CodexWorker::new(cfg);
    let first = thread(&say(&mut w, "one"));
    let flag = Arc::new(AtomicBool::new(false));
    let setter = {
        let flag = flag.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::SeqCst);
        })
    };
    let r = w.polish(&message("hang"), &|| flag.load(Ordering::SeqCst));
    setter.join().unwrap();
    assert_eq!(r, Err(PolishError::Cancelled));
    let after = thread(&say(&mut w, "two"));
    assert_ne!(process(&after), process(&first));
    w.shutdown();
}

/// Polishes with a cancel flag that flips after 300 ms; returns the result and how long it took.
fn polish_cancelled_after_300ms(w: &mut CodexWorker) -> (Result<String, PolishError>, Duration) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let flag = Arc::new(AtomicBool::new(false));
    let setter = {
        let flag = flag.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::SeqCst);
        })
    };
    let t0 = Instant::now();
    let r = w.polish(&message("after the respawn"), &|| {
        flag.load(Ordering::SeqCst)
    });
    setter.join().unwrap();
    (r, t0.elapsed())
}

#[test]
fn a_cancel_during_the_respawn_warm_up_returns_fast_and_does_not_pause() {
    let sb = Sandbox::new();
    // The first start answers; every later warm-up hangs. warm_up is 10 s, so only the cancel can
    // end the wait.
    let mut w = CodexWorker::new(sb.codex(&[("SHEPIT_FAKE_HANG_WARM_FROM", "2")]));
    say(&mut w, "one");
    assert!(matches!(ask(&mut w, "exit"), Err(PolishError::Exited(_))));
    // Three cancelled respawns: FAILURES_BEFORE_PAUSE of them would pause the worker if they counted.
    for _ in 0..3 {
        let (r, took) = polish_cancelled_after_300ms(&mut w);
        assert_eq!(r, Err(PolishError::Cancelled));
        assert!(took < Duration::from_secs(2), "{took:?}");
    }
    w.shutdown();
}

#[test]
fn the_source_is_codex() {
    let sb = Sandbox::new();
    let w = CodexWorker::new(sb.codex(&[]));
    assert_eq!(w.source(), shepit_core::polish::route::Source::Codex);
}
