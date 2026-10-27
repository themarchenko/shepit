mod support;

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

use shepit_core::app_version::is_newer;
use shepit_core::cleanup::passthrough_clean;
use shepit_core::dictionary::{correct, new_entries};
use shepit_core::gesture::{HotkeyGesture, Output};
use shepit_core::live::split;
use shepit_core::polish::pipeline::{format_request, PolishError, Polisher};
use shepit_core::polish::prompt::{user_message, FormatRequest, PromptContext};
use shepit_core::polish::reply::{finish, parse_reply};
use shepit_core::polish::route::Source;
use shepit_core::profile::{profile_by_id, resolve_mode, resolve_profile, word_count, Mode};
use shepit_core::speech::{is_junk, join, retry_language};

/// Fixture files that have a Rust runner in this file.
const PORTED: [&str; 16] = [
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
    "live-preview-split.json",
    "prompt-message.json",
    "reply-parse.json",
    "reply-finish.json",
    "polish-routing.json",
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
    let f = support::load::<DictionaryLearningInput, Vec<String>>("dictionary-learning.json", 8)
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

#[derive(Deserialize)]
struct SplitInput {
    frozen: String,
    current: String,
}

#[derive(Deserialize)]
struct SplitExpected {
    stable: String,
    unstable: String,
}

#[test]
fn live_preview_split_matches_shared_fixture() {
    let f = support::load::<SplitInput, SplitExpected>("live-preview-split.json", 10).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            let got = split(&c.input.frozen, &c.input.current);
            got.stable != c.expected.stable || got.unstable != c.expected.unstable
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct PromptExample {
    raw: String,
    #[serde(rename = "final")]
    final_text: String,
}

#[derive(Deserialize)]
struct PromptContextInput {
    app: Option<String>,
    bundle_id: Option<String>,
    window_title: Option<String>,
    url: Option<String>,
    text_before_caret: Option<String>,
    selected_text: Option<String>,
}

#[derive(Deserialize)]
struct PromptMessageInput {
    transcript: String,
    language_hint: Option<String>,
    mode: String,
    profile: String,
    context: PromptContextInput,
    dictionary: Option<Vec<String>>,
    examples: Option<Vec<PromptExample>>,
}

fn request_from(i: &PromptMessageInput) -> FormatRequest {
    FormatRequest {
        transcript: i.transcript.clone(),
        language_hint: i.language_hint.clone(),
        mode: Mode::from_id(&i.mode).expect("known mode"),
        profile: profile_by_id(&i.profile).expect("known profile"),
        context: PromptContext {
            app: i.context.app.clone(),
            bundle_id: i.context.bundle_id.clone(),
            window_title: i.context.window_title.clone(),
            url: i.context.url.clone(),
            text_before_caret: i.context.text_before_caret.clone(),
            selected_text: i.context.selected_text.clone(),
        },
        dictionary: i.dictionary.clone().unwrap_or_default(),
        examples: i
            .examples
            .iter()
            .flatten()
            .map(|e| (e.raw.clone(), e.final_text.clone()))
            .collect(),
        secure: false,
    }
}

#[test]
fn prompt_message_matches_shared_fixture() {
    let f = support::load::<PromptMessageInput, String>("prompt-message.json", 12).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| user_message(&request_from(&c.input)) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct ReplyInput {
    reply: String,
}

#[derive(Deserialize, PartialEq, Debug)]
struct ParsedExpected {
    text: String,
    language: Option<String>,
    new_terms: Vec<String>,
}

#[test]
fn reply_parse_matches_shared_fixture() {
    let f = support::load::<ReplyInput, Option<ParsedExpected>>("reply-parse.json", 14).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            let got = parse_reply(&c.input.reply).map(|p| ParsedExpected {
                text: p.text,
                language: p.language,
                new_terms: p.new_terms,
            });
            got != c.expected
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct FinishInput {
    text: String,
    profile: String,
}

#[test]
fn reply_finish_matches_shared_fixture() {
    let f = support::load::<FinishInput, String>("reply-finish.json", 9).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            let profile = profile_by_id(&c.input.profile).expect("known profile");
            finish(&c.input.text, profile) != c.expected
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[derive(Deserialize)]
struct RoutingInput {
    mode: String,
    profile: String,
    transcript: String,
    fast_path_max_words: i64,
    dictionary: Vec<String>,
    agent: String,
    agent_text: Option<String>,
    #[serde(default)]
    secure: bool,
}

#[derive(Deserialize, PartialEq, Debug)]
struct RoutingExpected {
    source: String,
    text: String,
}

/// Answers with `agent_text` wrapped in the JSON reply the prompt asks for, or fails.
struct StubAgent {
    reply: Result<String, PolishError>,
}

impl Polisher for StubAgent {
    fn source(&self) -> Source {
        Source::Claude
    }
    fn polish(&mut self, _: &str, _: &dyn Fn() -> bool) -> Result<String, PolishError> {
        self.reply.clone()
    }
}

#[test]
fn polish_routing_matches_shared_fixture() {
    let f = support::load::<RoutingInput, RoutingExpected>("polish-routing.json", 11).unwrap();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            let i = &c.input;
            let request = FormatRequest {
                transcript: i.transcript.clone(),
                language_hint: None,
                mode: Mode::from_id(&i.mode).expect("known mode"),
                profile: profile_by_id(&i.profile).expect("known profile"),
                context: PromptContext::default(),
                dictionary: i.dictionary.clone(),
                examples: Vec::new(),
                secure: i.secure,
            };
            let mut agent = match i.agent.as_str() {
                "none" => None,
                "reply" => {
                    let text = i.agent_text.clone().unwrap_or_default();
                    let reply =
                        serde_json::json!({"text": text, "language": "en", "new_terms": []});
                    Some(StubAgent {
                        reply: Ok(reply.to_string()),
                    })
                }
                "error" => Some(StubAgent {
                    reply: Err(PolishError::Timeout),
                }),
                other => panic!("{}: agent {other}", c.name),
            };
            let polisher = agent.as_mut().map(|a| a as &mut dyn Polisher);
            let got = format_request(&request, i.fast_path_max_words, polisher, &|| false);
            RoutingExpected {
                source: got.source.as_str().to_string(),
                text: got.text,
            } != c.expected
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[test]
fn every_fixture_file_has_the_envelope() {
    let files = json_files();
    assert!(files.len() >= 16, "only {} fixture files", files.len());
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
