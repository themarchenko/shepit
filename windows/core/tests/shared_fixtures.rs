mod support;

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

use shepit_core::app_version::is_newer;
use shepit_core::cleanup::passthrough_clean;
use shepit_core::dictionary::{correct, new_entries};
use shepit_core::gesture::{HotkeyGesture, Output};
use shepit_core::profile::{profile_by_id, resolve_mode, resolve_profile, word_count, Mode};
use shepit_core::speech::{is_junk, join, retry_language};

/// Fixture files that have a Rust runner in this file.
const PORTED: [&str; 11] = [
    "app-version.json",
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

/// Fixture files whose Rust port is still to do. Move a name to PORTED with its runner.
const PENDING: [&str; 0] = [];

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

#[derive(Deserialize)]
struct DictionaryCorrectionInput {
    text: String,
    dictionary: Vec<String>,
}

#[test]
fn dictionary_correction_matches_shared_fixture() {
    let f = support::load::<DictionaryCorrectionInput, String>("dictionary-correction.json", 23)
        .unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| correct(&c.input.text, &c.input.dictionary) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct DictionaryLearningInput {
    reported: Vec<String>,
    existing: Vec<String>,
}

#[test]
fn dictionary_learning_matches_shared_fixture() {
    let f = support::load::<DictionaryLearningInput, Vec<String>>("dictionary-learning.json", 1)
        .unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| new_entries(&c.input.reported, &c.input.existing) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct HotkeyEvent {
    key: String,
    at: f64,
}

#[derive(Deserialize)]
struct HotkeyInput {
    events: Vec<HotkeyEvent>,
}

#[derive(Deserialize)]
struct HotkeyExpected {
    output: String,
    duration: Option<f64>,
}

#[test]
fn hotkey_gestures_match_shared_fixture() {
    let f = support::load::<HotkeyInput, Vec<HotkeyExpected>>("hotkey-gestures.json", 4).unwrap();
    let mut failing: Vec<String> = Vec::new();
    for c in &f.cases {
        if c.input.events.len() != c.expected.len() {
            failing.push(format!("{}: one expected output per event", c.name));
            continue;
        }
        let mut gesture = HotkeyGesture::new();
        for (i, (event, want)) in c.input.events.iter().zip(&c.expected).enumerate() {
            let out = match event.key.as_str() {
                "down" => gesture.key_down(event.at),
                "up" => gesture.key_up(event.at),
                other => {
                    failing.push(format!("{} event {i}: unknown key {other}", c.name));
                    continue;
                }
            };
            let (label, duration) = match out {
                Output::None => ("none", None),
                Output::PressBegan => ("press_began", None),
                Output::PressEnded { duration } => ("press_ended", Some(duration)),
                Output::ShortTap => ("short_tap", None),
                Output::DoubleTap => ("double_tap", None),
            };
            if label != want.output {
                failing.push(format!("{} event {i}: {label} != {}", c.name, want.output));
            }
            match (want.duration, duration) {
                (Some(w), Some(d)) if (w - d).abs() <= 0.001 => {}
                (None, None) => {}
                (w, d) => failing.push(format!("{} event {i} duration: {d:?} != {w:?}", c.name)),
            }
        }
    }
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct TextInput {
    text: String,
}

#[test]
fn passthrough_clean_matches_shared_fixture() {
    let f = support::load::<TextInput, String>("passthrough-clean.json", 5).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| passthrough_clean(&c.input.text) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[test]
fn word_count_matches_shared_fixture() {
    let f = support::load::<TextInput, usize>("word-count.json", 4).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| word_count(&c.input.text) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct ProfileInput {
    bundle_id: Option<String>,
    window_title: Option<String>,
    url: Option<String>,
}

#[test]
fn profile_resolution_matches_shared_fixture() {
    let f = support::load::<ProfileInput, String>("profile-resolution.json", 60).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            resolve_profile(
                c.input.bundle_id.as_deref(),
                c.input.window_title.as_deref(),
                c.input.url.as_deref(),
            )
            .id != c.expected
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct ModeInput {
    requested: String,
    profile: String,
    word_count: usize,
    prompt_mode_min_words: usize,
}

#[test]
fn mode_resolution_matches_shared_fixture() {
    let f = support::load::<ModeInput, String>("mode-resolution.json", 5).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            let requested = Mode::from_id(&c.input.requested).expect("known mode");
            let profile = profile_by_id(&c.input.profile).expect("known profile");
            resolve_mode(
                requested,
                profile,
                c.input.word_count,
                c.input.prompt_mode_min_words,
            )
            .as_str()
                != c.expected
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[test]
fn hallucination_filter_matches_shared_fixture() {
    let f = support::load::<TextInput, bool>("hallucination-filter.json", 25).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| is_junk(&c.input.text) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct RetryInput {
    detected: Option<String>,
    text: String,
}

#[test]
fn retry_language_matches_shared_fixture() {
    let f = support::load::<RetryInput, Option<String>>("retry-language.json", 6).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| retry_language(c.input.detected.as_deref(), &c.input.text) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct JoinInput {
    a: String,
    b: String,
}

#[test]
fn live_preview_join_matches_shared_fixture() {
    let f = support::load::<JoinInput, String>("live-preview-join.json", 3).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| join(&c.input.a, &c.input.b) != c.expected)
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
