mod support;

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

use shepit_core::app_version::is_newer;

/// Fixture files that have a Rust runner in this file.
const PORTED: [&str; 1] = ["app-version.json"];

/// Fixture files whose Rust port is still to do. Move a name to PORTED with its runner.
const PENDING: [&str; 10] = [
    "dictionary-correction.json",
    "dictionary-learning.json",
    "hotkey-gestures.json",
    "passthrough-clean.json",
    "word-count.json",
    "profile-resolution.json",
    "mode-resolution.json",
    "hallucination-filter.json",
    "retry-language.json",
    "live-preview-join.json",
];

fn json_files() -> BTreeSet<String> {
    std::fs::read_dir(support::fixtures_dir())
        .expect("fixtures dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect()
}

#[derive(Deserialize)]
struct VersionInput {
    candidate: String,
    installed: String,
}

#[test]
fn app_version_matches_shared_fixture() {
    let f = support::load::<VersionInput, bool>("app-version.json", 7).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| is_newer(&c.input.candidate, &c.input.installed) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[test]
fn every_fixture_file_has_the_envelope() {
    let files = json_files();
    assert!(files.len() >= 11, "only {} fixture files", files.len());
    for name in &files {
        support::load::<Value, Value>(name, 1).unwrap();
    }
}

#[test]
fn every_fixture_file_is_ported_or_pending() {
    let ported: BTreeSet<String> = PORTED.iter().map(|s| s.to_string()).collect();
    let pending: BTreeSet<String> = PENDING.iter().map(|s| s.to_string()).collect();
    assert!(ported.is_disjoint(&pending), "a file is in both lists");
    let union: BTreeSet<String> = ported.union(&pending).cloned().collect();
    assert_eq!(json_files(), union);
}

#[test]
fn loader_rejects_a_missing_file() {
    let err = support::load::<Value, Value>("no-such-fixture.json", 1).unwrap_err();
    assert!(err.contains("missing fixture"), "{err}");
}

#[test]
fn loader_rejects_a_zero_minimum() {
    assert!(support::load::<Value, Value>("app-version.json", 0).is_err());
}

#[test]
fn loader_rejects_too_few_cases() {
    assert!(support::load::<Value, Value>("app-version.json", 8).is_err());
}

#[test]
fn loader_rejects_an_unsupported_version() {
    let path = std::env::temp_dir().join("shepit-fixture-version-2.json");
    std::fs::write(
        &path,
        r#"{"version":2,"description":"x","cases":[{"name":"A/1","input":{},"expected":1}]}"#,
    )
    .unwrap();
    let result = support::load_from::<Value, Value>(Path::new(&path), 1);
    let _ = std::fs::remove_file(&path);
    assert!(result.is_err());
}
