//! The same flows through a `.cmd` shim in a folder whose name needs quoting. npm installs Claude
//! Code and Codex as `.cmd` files, and the user's profile path can hold spaces and Cyrillic, so
//! these prove that nothing in argv or on the pipes is mangled on the way. Windows only: nothing
//! here runs on the Mac.
#![cfg(windows)]
mod common;

use common::*;
use serde_json::json;
use shepit_agent::{creation_flags, ClaudeWorker, CodexWorker, CREATE_NO_WINDOW};
use shepit_core::polish::pipeline::{PolishError, Polisher};
use shepit_core::polish::{claude_wire, codex_wire};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const FOLDER: &str = "shepit agent & 100% тест";

/// A shim `<root>/<FOLDER>/agent.cmd` that forwards to the fake agent. The work and output
/// folders live in the same odd-named folder, so the paths inside argv need the quoting too.
struct Shim {
    sb: Sandbox,
    cmd: PathBuf,
    out: PathBuf,
    work: PathBuf,
}

fn shim() -> Shim {
    let sb = Sandbox::new();
    let dir = sb.root.path().join(FOLDER);
    let out = dir.join("out");
    let work = dir.join("work");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    let cmd = dir.join("agent.cmd");
    std::fs::write(&cmd, format!("@\"{FAKE}\" %*\r\n")).unwrap();
    Shim { sb, cmd, out, work }
}

fn env(s: &Shim, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut e = vec![(
        "SHEPIT_FAKE_DIR".to_string(),
        s.out.to_string_lossy().into_owned(),
    )];
    e.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    e
}

fn claude(s: &Shim, extra: &[(&str, &str)]) -> ClaudeWorker {
    let mut c = s.sb.claude_with(&s.cmd, &[]);
    c.work_dir = s.work.clone();
    c.extra_env = env(s, extra);
    ClaudeWorker::new(c)
}

fn codex(s: &Shim, extra: &[(&str, &str)]) -> CodexWorker {
    let mut c = s.sb.codex_with(&s.cmd, &[]);
    c.work_dir = s.work.clone();
    c.extra_env = env(s, extra);
    CodexWorker::new(c)
}

fn read_json(s: &Shim, name: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(s.out.join(name)).unwrap()).unwrap()
}

#[test]
fn the_claude_argv_arrives_exactly() {
    let s = shim();
    let mut w = claude(&s, &[]);
    w.polish(&message("x"), &no_cancel).unwrap();
    let expected = claude_wire::args(
        "haiku",
        "low",
        &s.work.join("system-prompt.md").to_string_lossy(),
        &s.work.join("worker-debug.log").to_string_lossy(),
    );
    // Holds the empty `--tools ""` and `--setting-sources ""`.
    assert_eq!(read_json(&s, "argv.json"), json!(expected));
    w.shutdown();
}

#[test]
fn the_codex_argv_arrives_exactly() {
    let s = shim();
    let mut w = codex(&s, &[]);
    w.polish(&message("x"), &no_cancel).unwrap();
    // Holds the quoted values: `sandbox_mode="read-only"`.
    assert_eq!(read_json(&s, "argv.json"), json!(codex_wire::arguments()));
    w.shutdown();
}

#[test]
fn cyrillic_survives_stdin_and_stdout_through_the_shim() {
    let s = shim();
    let mut c = claude(&s, &[]);
    let t = reply_text(
        &c.polish(&message("Привіт, світе! ґєї"), &no_cancel)
            .unwrap(),
    );
    assert!(t.ends_with("] Привіт, світе! ґєї"), "{t}");
    c.shutdown();
    let mut x = codex(&s, &[]);
    let t = reply_text(
        &x.polish(&message("Привіт, світе! ґєї"), &no_cancel)
            .unwrap(),
    );
    assert!(t.ends_with("] Привіт, світе! ґєї"), "{t}");
    x.shutdown();
}

#[test]
fn the_flows_hold_through_the_shim() {
    let s = shim();
    let mut w = claude(&s, &[]);
    let first = tag_of(&reply_text(&w.polish(&message("one"), &no_cancel).unwrap()));
    assert!(matches!(
        w.polish(&message("exit"), &no_cancel),
        Err(PolishError::Exited(_))
    ));
    let after = tag_of(&reply_text(&w.polish(&message("two"), &no_cancel).unwrap()));
    assert_ne!(after, first);
    w.shutdown();
}

/// cmd.exe starts the fake, which starts a grandchild that keeps writing a heartbeat. Killing only
/// cmd.exe would leave both running; the Job object must end the tree.
#[test]
fn a_timeout_kills_the_whole_tree() {
    let s = shim();
    let mut w = claude(&s, &[("SHEPIT_FAKE_GRANDCHILD", "1")]); // request timeout: 1 s
    w.polish(&message("one"), &no_cancel).unwrap();
    assert_eq!(
        w.polish(&message("hang"), &no_cancel),
        Err(PolishError::Timeout)
    );
    let beat = s.out.join("grandchild-heartbeat.txt");
    assert!(
        beat.exists(),
        "the grandchild never started, so the test proves nothing"
    );
    std::thread::sleep(Duration::from_secs(2));
    let size = |p: &PathBuf| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let before = size(&beat);
    let t0 = Instant::now();
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(
        size(&beat),
        before,
        "the grandchild was still writing {:?} after the kill",
        t0.elapsed()
    );
    w.shutdown();
}

#[test]
fn the_agent_is_spawned_without_a_console_window() {
    assert_eq!(creation_flags() & CREATE_NO_WINDOW, CREATE_NO_WINDOW);
    let s = shim();
    let mut w = claude(&s, &[]);
    w.polish(&message("x"), &no_cancel).unwrap();
    assert_eq!(
        std::fs::read_to_string(s.out.join("console-window.txt")).unwrap(),
        "none"
    );
    w.shutdown();
}
