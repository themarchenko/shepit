//! The updater trusts the key pinned in tauri.conf.json and nothing else. These checks read the
//! source, the manifest and the lock file, so a later change that overrides the key, the endpoint
//! or the version rule fails here before it ships. The behaviour is tested in src/updater.rs.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every src/*.rs with the text before its first `#[cfg(test)]`.
fn sources() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root().join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = read(&path);
        let code = text.split("#[cfg(test)]").next().unwrap().to_string();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        out.push((name, code, text));
    }
    out
}

#[test]
fn the_scan_finds_the_sources() {
    let names: Vec<String> = sources().into_iter().map(|(n, _, _)| n).collect();
    for expected in ["main.rs", "updater.rs", "tray.rs"] {
        assert!(names.contains(&expected.to_string()), "{names:?}");
    }
}

#[test]
fn the_app_never_overrides_the_updater_trust_in_code() {
    let forbidden = [
        ".pubkey(",
        ".endpoints(",
        "dangerous",
        "allow_downgrades",
        "restore_update",
        "installer_arg",
        "configure_client",
        ".install(",
    ];
    for (name, code, _) in sources() {
        for word in forbidden {
            assert!(!code.contains(word), "{name} uses {word}");
        }
    }
}

#[test]
fn only_updater_rs_talks_to_the_plugin() {
    for (name, _, text) in sources() {
        if name == "updater.rs" {
            continue;
        }
        assert!(
            !text.contains("tauri_plugin_updater::"),
            "{name} uses the plugin directly"
        );
    }
}

#[test]
fn main_registers_the_updater_plugin_the_one_way() {
    let main = read(&root().join("src/main.rs"));
    assert!(main.contains(".plugin(updater::plugin())"));
    assert_eq!(main.matches("updater::plugin()").count(), 1);
}

#[test]
fn the_lock_file_pins_the_audited_versions() {
    let lock = read(&root().join("../../Cargo.lock"));
    let versions_of = |name: &str| -> Vec<String> {
        lock.split("[[package]]")
            .filter(|block| {
                block
                    .lines()
                    .any(|l| l.trim() == format!("name = \"{name}\""))
            })
            .filter_map(|block| {
                block
                    .lines()
                    .find_map(|l| l.strip_prefix("version = \""))
                    .map(|v| v.trim_end_matches('"').to_string())
            })
            .collect()
    };
    assert_eq!(versions_of("tauri"), ["2.12.5"]);
    assert_eq!(versions_of("tauri-plugin-updater"), ["2.14.0"]);
    // One copy, so the plugin verifies with the version release-check audits.
    assert_eq!(versions_of("minisign-verify"), ["0.3.0"]);
}

#[test]
fn the_manifest_pins_the_plugin_exactly() {
    let manifest = read(&root().join("Cargo.toml"));
    assert!(
        manifest
            .lines()
            .any(|l| l.trim() == "tauri-plugin-updater = \"=2.14.0\""),
        "tauri-plugin-updater must be pinned with ="
    );
}
