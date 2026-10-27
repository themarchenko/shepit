//! Stand-in for `claude` and `codex app-server`, used by the shepit-agent tests (a port of the fake
//! in CodexFormatterTests.swift). It acts as Codex when argv holds `app-server`, as Claude
//! otherwise. Files go to `$SHEPIT_FAKE_DIR`, or next to the executable when that is unset:
//!
//! - `starts.log`: one line per start, written before anything else, even when it dies.
//! - `argv.json`: the arguments, as a JSON array of strings.
//! - `system-prompt-seen.md` (Claude): the bytes of the file named by `--system-prompt-file`.
//! - `thread-start.json` (Codex): the params of the last `thread/start`.
//! - `console-window.txt` (Windows): `none` or `present`, from GetConsoleWindow.
//! - `grandchild-heartbeat.txt`: one byte per 100 ms while the grandchild lives.
//!
//! Environment: `SHEPIT_FAKE_START=die` exits with code 1 at start. `SHEPIT_FAKE_GRANDCHILD=1` makes
//! `hang` first spawn a grandchild that inherits stdout and keeps writing the heartbeat, so a test
//! can tell whether the whole process tree was killed.
//!
//! Transcripts (the text after `Transcript:\n`, or the whole message): `hang` never answers, `exit`
//! exits with code 3, `fail` is an error result (Claude) or a failed turn (Codex), `bad` answers
//! with text that is not JSON (Claude), `approve` asks for a command approval (Codex). Anything else
//! is echoed as `[<pid>] <text>` (Claude) or `[thr_<pid>_<n>] <text>` (Codex). A message that starts
//! with `Warm-up` is answered with `ok`.

use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

fn out_dir() -> PathBuf {
    match std::env::var_os("SHEPIT_FAKE_DIR") {
        Some(d) => PathBuf::from(d),
        None => std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_default(),
    }
}

fn send(value: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn transcript_of(text: &str) -> &str {
    text.rsplit_once("Transcript:\n").map_or(text, |(_, t)| t)
}

fn spawn_grandchild() {
    if std::env::var_os("SHEPIT_FAKE_GRANDCHILD").is_none() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .env("SHEPIT_FAKE_SLEEP", "1")
            .stdin(std::process::Stdio::null())
            .spawn();
    }
}

fn heartbeat() {
    let path = out_dir().join("grandchild-heartbeat.txt");
    for _ in 0..300 {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = f.write_all(b".");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn console_window_report() {
    extern "system" {
        fn GetConsoleWindow() -> isize;
    }
    // SAFETY: GetConsoleWindow takes no arguments and only reads process state.
    let none = unsafe { GetConsoleWindow() } == 0;
    let _ = std::fs::write(
        out_dir().join("console-window.txt"),
        if none { "none" } else { "present" },
    );
}

#[cfg(not(windows))]
fn console_window_report() {}

fn main() {
    if std::env::var_os("SHEPIT_FAKE_SLEEP").is_some() {
        heartbeat();
        return;
    }
    let dir = out_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("starts.log"))
    {
        let _ = writeln!(f, "start {}", std::process::id());
    }
    if std::env::var("SHEPIT_FAKE_START").as_deref() == Ok("die") {
        std::process::exit(1);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let _ = std::fs::write(dir.join("argv.json"), json!(args).to_string());
    console_window_report();
    if args.iter().any(|a| a == "app-server") {
        codex(&dir);
    } else {
        claude(&dir, &args);
    }
}

fn claude(dir: &std::path::Path, args: &[String]) {
    if let Some(i) = args.iter().position(|a| a == "--system-prompt-file") {
        if let Some(bytes) = args.get(i + 1).and_then(|p| std::fs::read(p).ok()) {
            let _ = std::fs::write(dir.join("system-prompt-seen.md"), bytes);
        }
    }
    let pid = std::process::id();
    let result = |text: &str, is_error: bool| {
        // A line that is not JSON and an event of another type come first, as the real CLI emits.
        println!("debug: not json");
        send(&json!({"type": "system", "subtype": "init"}));
        send(&json!({"type": "assistant", "message": {"content": []}}));
        send(&json!({
            "type": "result",
            "subtype": if is_error { "error_during_execution" } else { "success" },
            "is_error": is_error,
            "result": text,
            "usage": {"input_tokens": 1, "output_tokens": 1},
        }));
    };
    let reply = |text: &str| json!({"text": text, "language": "en", "new_terms": []}).to_string();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let content = message["message"]["content"].as_str().unwrap_or("");
        let transcript = transcript_of(content);
        if content.starts_with("Warm-up") {
            result(&reply("ok"), false);
        } else if transcript == "hang" {
            spawn_grandchild();
        } else if transcript == "exit" {
            std::process::exit(3);
        } else if transcript == "fail" {
            result("You've hit your usage limit.", true);
        } else if transcript == "bad" {
            result("this is not json", false);
        } else {
            result(&reply(&format!("[{pid}] {transcript}")), false);
        }
    }
}

fn codex(dir: &std::path::Path) {
    let pid = std::process::id();
    let complete = |thread_id: &str, turn_id: &str, text: &str| {
        let reply = json!({"text": text, "language": "en", "new_terms": []}).to_string();
        send(&json!({"method": "item/completed", "params": {
            "threadId": thread_id, "turnId": turn_id,
            "item": {"id": format!("msg_{turn_id}"), "type": "agentMessage", "text": reply,
                     "phase": "final_answer"}}}));
        send(&json!({"method": "turn/completed", "params": {
            "threadId": thread_id,
            "turn": {"id": turn_id, "status": "completed", "items": [], "error": null}}}));
    };
    let mut threads = 0u32;
    let mut turns = 0u32;
    let mut waiting: Option<(String, String)> = None;
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // The answer to our approval request.
            if let Some((thread_id, turn_id)) = waiting.take() {
                let decision = message["result"]["decision"]
                    .as_str()
                    .unwrap_or("no decision");
                complete(&thread_id, &turn_id, decision);
            }
            continue;
        };
        let id = message.get("id").cloned();
        match method {
            "initialize" => send(&json!({"id": id, "result": {"userAgent": "fake-codex"}})),
            "thread/start" => {
                threads += 1;
                let thread_id = format!("thr_{pid}_{threads}");
                let _ =
                    std::fs::write(dir.join("thread-start.json"), message["params"].to_string());
                send(
                    &json!({"id": id, "result": {"thread": {"id": thread_id, "ephemeral": true}}}),
                );
                send(&json!({"method": "thread/started", "params": {"thread": {"id": thread_id}}}));
            }
            "turn/start" => {
                turns += 1;
                let params = &message["params"];
                let thread_id = params["threadId"].as_str().unwrap_or("").to_string();
                let turn_id = format!("turn_{turns}");
                let text = params["input"][0]["text"].as_str().unwrap_or("");
                let turn =
                    json!({"id": turn_id, "status": "inProgress", "items": [], "error": null});
                send(&json!({"id": id, "result": {"turn": turn}}));
                send(
                    &json!({"method": "turn/started", "params": {"threadId": thread_id, "turn": turn}}),
                );
                let transcript = transcript_of(text);
                if text.starts_with("Warm-up") {
                    complete(&thread_id, &turn_id, "ok");
                } else if transcript == "hang" {
                    spawn_grandchild();
                } else if transcript == "exit" {
                    std::process::exit(3);
                } else if transcript == "fail" {
                    let error = json!({"message": "You've hit your usage limit.",
                                       "codexErrorInfo": "usageLimitExceeded"});
                    send(&json!({"method": "error", "params": {
                        "threadId": thread_id, "turnId": turn_id, "willRetry": false,
                        "error": error}}));
                    send(&json!({"method": "turn/completed", "params": {
                        "threadId": thread_id,
                        "turn": {"id": turn_id, "status": "failed", "items": [], "error": error}}}));
                } else if transcript == "approve" {
                    waiting = Some((thread_id.clone(), turn_id.clone()));
                    send(
                        &json!({"method": "item/commandExecution/requestApproval", "id": 0,
                        "params": {"threadId": thread_id, "turnId": turn_id,
                                   "itemId": "item_1", "command": "touch x"}}),
                    );
                } else {
                    complete(&thread_id, &turn_id, &format!("[{thread_id}] {transcript}"));
                }
            }
            _ if id.is_some() => send(&json!({"id": id, "result": {}})),
            _ => {}
        }
    }
}
