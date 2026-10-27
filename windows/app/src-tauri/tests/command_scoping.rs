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

/// Every capabilities/*.json, parsed, with its file name.
fn capabilities() -> Vec<(String, serde_json::Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let text = fs::read_to_string(&p).unwrap();
            let json =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            (p.file_name().unwrap().to_string_lossy().into_owned(), json)
        })
        .collect()
}

fn permissions(capability: &serde_json::Value) -> Vec<String> {
    capability["permissions"]
        .as_array()
        .expect("permissions array")
        .iter()
        .map(|p| p.as_str().expect("permission string").to_string())
        .collect()
}

fn permission_of(command: &str) -> String {
    format!("allow-{}", command.replace('_', "-"))
}

const CARD_COMMANDS: [&str; 3] = ["update_card_ready", "update_action", "update_resize"];

#[test]
fn every_command_is_granted_to_a_window() {
    let granted: Vec<String> = capabilities()
        .iter()
        .flat_map(|(_, c)| permissions(c))
        .collect();
    for name in command_names() {
        let permission = permission_of(&name);
        assert!(
            granted.contains(&permission),
            "\"{permission}\" is in no capability file"
        );
    }
}

#[test]
fn the_scan_reads_every_capability_file() {
    let names: Vec<String> = capabilities().into_iter().map(|(n, _)| n).collect();
    for expected in ["default.json", "settings.json", "update.json"] {
        assert!(names.contains(&expected.to_string()), "{names:?}");
    }
}

#[test]
fn the_update_commands_are_in_the_scan() {
    let names = command_names();
    for name in [
        "get_update_status",
        "check_updates_now",
        "show_update_card",
        "update_card_ready",
        "update_action",
        "update_resize",
    ] {
        assert!(names.contains(&name.to_string()), "{name} is not a command");
    }
}

#[test]
fn the_update_window_gets_only_the_card_commands() {
    let caps = capabilities();
    let (_, update) = caps
        .iter()
        .find(|(n, _)| n == "update.json")
        .expect("capabilities/update.json");
    assert_eq!(update["windows"], serde_json::json!(["update"]));
    let mut got = permissions(update);
    got.sort();
    let mut want: Vec<String> = CARD_COMMANDS.iter().map(|c| permission_of(c)).collect();
    want.push("core:event:default".into());
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn the_card_commands_are_granted_to_no_other_window() {
    for (file, cap) in capabilities() {
        if file == "update.json" {
            continue;
        }
        for command in CARD_COMMANDS {
            assert!(
                !permissions(&cap).contains(&permission_of(command)),
                "{file} grants {command}"
            );
        }
    }
}

#[test]
fn the_settings_window_gets_the_update_commands_but_not_the_card_ones() {
    let caps = capabilities();
    let (_, settings) = caps.iter().find(|(n, _)| n == "settings.json").unwrap();
    let got = permissions(settings);
    for command in ["get_update_status", "check_updates_now", "show_update_card"] {
        assert!(got.contains(&permission_of(command)), "{command}");
    }
}

#[test]
fn no_window_can_call_a_plugin_command() {
    // The updater plugin registers check/download/install for JS. Its permissions, and those of
    // the plugins that could open or run things, are granted to no window: every call is Rust.
    let banned = ["updater:", "opener:", "shell:", "process:", "fs:", "http:"];
    for (file, cap) in capabilities() {
        for permission in permissions(&cap) {
            for prefix in banned {
                assert!(
                    !permission.starts_with(prefix),
                    "{file} grants {permission}"
                );
            }
        }
    }
}
