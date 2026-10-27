//! The signed-version rule of tauri-plugin-updater 2.14.0 (updater.rs:1926-1958). `key-c.pub` and
//! `payload-v0.3.1.sig` come from `tauri signer sign --app-version 0.3.1`, which is what
//! `tauri build` does for the updater artifact (the CLI help says so). Throwaway key, private half
//! not committed.

use shepit_release_check::*;
use std::fs;
use std::path::PathBuf;

fn fixture_text(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(p).unwrap().trim().to_string()
}
fn payload() -> Vec<u8> {
    fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/payload.bin")).unwrap()
}
fn versioned_feed(tag: &str) -> String {
    build_feed(
        tag,
        "",
        "2026-10-27T00:00:00Z",
        &fixture_text("payload-v0.3.1.sig"),
    )
}

#[test]
fn a_signature_for_the_announced_version_passes_even_when_required() {
    verify_feed_with(
        &versioned_feed("v0.3.1"),
        &payload(),
        "v0.3.1",
        &fixture_text("key-c.pub"),
        true,
    )
    .unwrap();
}

#[test]
fn a_signature_for_another_version_is_refused() {
    // A feed announcing 0.3.2 over the installer signed for 0.3.1 is the downgrade the field stops.
    for require in [false, true] {
        assert!(verify_feed_with(
            &versioned_feed("v0.3.2"),
            &payload(),
            "v0.3.2",
            &fixture_text("key-c.pub"),
            require
        )
        .is_err());
    }
}

#[test]
fn a_signature_without_a_version_passes_by_default_and_fails_when_required() {
    let feed = build_feed(
        "v0.3.1",
        "",
        "2026-10-27T00:00:00Z",
        &fixture_text("payload.bin.sig"),
    );
    let key = fixture_text("key-a.pub");
    verify_feed_with(&feed, &payload(), "v0.3.1", &key, false).unwrap();
    assert!(verify_feed_with(&feed, &payload(), "v0.3.1", &key, true).is_err());
}

#[test]
fn the_tag_must_be_plain_x_y_z() {
    for bad in ["0.3.1", "v0.3", "v0.3.1-rc1", "v0.3.1/../x", "v", "v1..2"] {
        assert!(version_of_tag(bad).is_err(), "{bad}");
    }
    assert_eq!(version_of_tag("v0.3.1").unwrap(), "0.3.1");
}

#[test]
fn the_check_config_rules_hold_for_the_repo_files() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../app/src-tauri");
    let read = |n: &str| serde_json::from_slice(&fs::read(dir.join(n)).unwrap()).unwrap();
    check_config(
        &read("tauri.conf.json"),
        &read("tauri.release.conf.json"),
        &read("tauri.runtime.conf.json"),
    )
    .unwrap();
}
