//! Every `#[tauri::command]` must be listed in build.rs COMMANDS and granted to some window in
//! capabilities/*.json. Without the manifest entry the command is open to every window, which is
//! how `detect_agents` once slipped through.

use std::fs;
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Names of the functions that follow a `#[tauri::command]` attribute.
fn command_names() -> Vec<String> {
    let mut names = Vec::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let mut armed = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("#[tauri::command") {
                armed = true;
            } else if armed && t.contains("fn ") {
                let after = t.split("fn ").nth(1).unwrap();
                let name: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                names.push(name);
                armed = false;
            }
        }
    }
    names
}

/// The string literals of the COMMANDS array in build.rs.
fn manifest_commands() -> Vec<String> {
    let build = read("build.rs");
    let body = build
        .split("COMMANDS: &[&str] = &[")
        .nth(1)
        .and_then(|s| s.split("];").next())
        .expect("COMMANDS array in build.rs");
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(String::from)
        .collect()
}

#[test]
fn the_scan_finds_the_commands() {
    let names = command_names();
    assert!(names.len() >= 10, "{names:?}");
    assert!(names.contains(&"widget_ready".to_string()), "{names:?}");
    assert!(names.contains(&"detect_agents".to_string()), "{names:?}");
}

#[test]
fn every_command_is_in_the_build_manifest() {
    let listed = manifest_commands();
    for name in command_names() {
        assert!(
            listed.contains(&name),
            "{name} is missing from COMMANDS in build.rs"
        );
    }
}

#[test]
fn every_command_is_granted_to_a_window() {
    let caps = format!(
        "{}{}",
        read("capabilities/default.json"),
        read("capabilities/settings.json")
    );
    for name in command_names() {
        let permission = format!("\"allow-{}\"", name.replace('_', "-"));
        assert!(
            caps.contains(&permission),
            "{permission} is in no capability file"
        );
    }
}
