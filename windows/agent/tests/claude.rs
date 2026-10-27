//! The Claude worker against the fake agent (the Claude half of ClaudeCodeWorker.swift's behaviour,
//! with the process flows of CodexFormatterProcessTests applied to it).
mod common;

use common::*;
use serde_json::json;
use shepit_agent::ClaudeWorker;
use shepit_core::polish::claude_wire;
use shepit_core::polish::pipeline::{PolishError, Polisher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn ask(w: &mut ClaudeWorker, transcript: &str) -> Result<String, PolishError> {
    w.polish(&message(transcript), &no_cancel)
}

fn say(w: &mut ClaudeWorker, transcript: &str) -> String {
    reply_text(&ask(w, transcript).unwrap())
}

#[test]
fn warm_up_then_format_echoes_the_transcript() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[]));
    w.warm_up().unwrap();
    let text = say(&mut w, "hello there");
    assert!(text.ends_with("] hello there"), "{text}");
    assert_eq!(
        sb.starts(),
        1,
        "the warm-up and the format share one process"
    );
    w.shutdown();
}

#[test]
fn a_format_without_a_warm_up_starts_and_warms_the_process_itself() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[]));
    assert!(say(&mut w, "first").ends_with("] first"));
    w.shutdown();
}

#[test]
fn argv_equals_the_core_args() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[]));
    say(&mut w, "x");
    let work = sb.work();
    let expected = claude_wire::args(
        "haiku",
        "low",
        &work.join("system-prompt.md").to_string_lossy(),
        &work.join("worker-debug.log").to_string_lossy(),
    );
    assert_eq!(sb.json("argv.json"), json!(expected));
    w.shutdown();
}

#[test]
fn the_system_prompt_file_is_utf8_without_a_bom() {
    let sb = Sandbox::new();
    let mut cfg = sb.claude(&[]);
    cfg.system_prompt = "Правила: привіт\nline two".to_string();
    let mut w = ClaudeWorker::new(cfg);
    say(&mut w, "x");
    let seen = std::fs::read(sb.out().join("system-prompt-seen.md")).unwrap();
    assert_eq!(seen, "Правила: привіт\nline two".as_bytes());
    w.shutdown();
}

#[test]
fn cyrillic_survives_stdin_and_stdout() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[]));
    let text = say(&mut w, "Привіт, світе! ґєї");
    assert!(text.ends_with("] Привіт, світе! ґєї"), "{text}");
    w.shutdown();
}

#[test]
fn a_hang_times_out_and_the_next_call_respawns() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[])); // request timeout: 1 s
    let first = tag_of(&say(&mut w, "one"));
    let t0 = Instant::now();
    assert_eq!(ask(&mut w, "hang"), Err(PolishError::Timeout));
    let took = t0.elapsed();
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_secs(5),
        "{took:?}"
    );
    let after = tag_of(&say(&mut w, "two"));
    assert_ne!(after, first, "a new process serves the next dictation");
    w.shutdown();
}

#[test]
fn an_exit_is_reported_and_recovered() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[]));
    say(&mut w, "one");
    assert!(matches!(ask(&mut w, "exit"), Err(PolishError::Exited(_))));
    assert!(say(&mut w, "two").ends_with("] two"));
    w.shutdown();
}

#[test]
fn an_error_result_replaces_the_process() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[]));
    let first = tag_of(&say(&mut w, "one"));
    assert_eq!(
        ask(&mut w, "fail"),
        Err(PolishError::Api("You've hit your usage limit.".into()))
    );
    let after = tag_of(&say(&mut w, "two"));
    assert_ne!(after, first, "any send error drops a Claude process");
    w.shutdown();
}

#[test]
fn a_bad_reply_keeps_the_same_process() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[]));
    let first = tag_of(&say(&mut w, "one"));
    // The worker hands the raw text on; the pipeline decides it is not a reply.
    assert_eq!(ask(&mut w, "bad").unwrap(), "this is not json");
    let after = tag_of(&say(&mut w, "two"));
    assert_eq!(after, first);
    w.shutdown();
}

#[test]
fn the_process_rotates_to_the_standby_at_the_turn_limit() {
    let sb = Sandbox::new();
    let mut cfg = sb.claude(&[]);
    cfg.timings.standby_after_turns = 2;
    cfg.timings.max_turns = 3;
    let mut w = ClaudeWorker::new(cfg);
    // The warm-up is turn 1, as on macOS.
    let a = tag_of(&say(&mut w, "a")); // turn 2: the standby starts
    assert!(wait_until(Duration::from_secs(10), || sb.starts() == 2));
    std::thread::sleep(Duration::from_millis(500)); // let the standby finish its warm-up
    let b = tag_of(&say(&mut w, "b")); // turn 3: the standby takes over after the reply
    let c = tag_of(&say(&mut w, "c"));
    assert_eq!(b, a);
    assert_ne!(c, a);
    w.shutdown();
}

#[test]
fn two_failed_starts_in_a_row_pause_starting() {
    let sb = Sandbox::new();
    let mut w = ClaudeWorker::new(sb.claude(&[("SHEPIT_FAKE_START", "die")]));
    assert!(ask(&mut w, "one").is_err());
    assert!(ask(&mut w, "two").is_err());
    assert_eq!(sb.starts(), 2);
    let t0 = Instant::now();
    assert_eq!(ask(&mut w, "three"), Err(PolishError::Paused));
    assert!(
        t0.elapsed() < Duration::from_millis(200),
        "{:?}",
        t0.elapsed()
    );
    assert_eq!(sb.starts(), 2, "a paused worker does not start a process");
}

#[test]
fn a_cancel_drops_the_process() {
    let sb = Sandbox::new();
    let mut cfg = sb.claude(&[]);
    cfg.timings.request = Duration::from_secs(20); // far past the test: only the cancel can end it
    let mut w = ClaudeWorker::new(cfg);
    let first = tag_of(&say(&mut w, "one"));
    let flag = Arc::new(AtomicBool::new(false));
    let setter = {
        let flag = flag.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::SeqCst);
        })
    };
    let t0 = Instant::now();
    let r = w.polish(&message("hang"), &|| flag.load(Ordering::SeqCst));
    setter.join().unwrap();
    assert_eq!(r, Err(PolishError::Cancelled));
    assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
    let after = tag_of(&say(&mut w, "two"));
    assert_ne!(
        after, first,
        "a cancelled request leaves the process in an unknown state"
    );
    w.shutdown();
}

#[test]
fn the_source_is_claude() {
    let sb = Sandbox::new();
    let w = ClaudeWorker::new(sb.claude(&[]));
    assert_eq!(w.source(), shepit_core::polish::route::Source::Claude);
}
