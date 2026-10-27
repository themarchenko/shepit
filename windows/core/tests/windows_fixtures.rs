//! Fixtures that only the Windows core runs (no Swift runner exists for them, so they stay out of
//! `shared/fixtures`).
mod support;

use serde::Deserialize;
use shepit_core::dictation::Phase;
use shepit_core::keys::{is_accepted, Hotkey, VK_ESCAPE};
use shepit_core::profile::{exe_tables, resolve_exe_profile};
use shepit_core::widget::{layout_for, LayoutInput, Style};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn windows_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[derive(Deserialize)]
struct ExeInput {
    exe: Option<String>,
    window_title: Option<String>,
    url: Option<String>,
}

type ExeFixture = support::Envelope<ExeInput, String>;

fn exe_fixture() -> ExeFixture {
    support::load_from(&windows_fixture("profile-resolution-windows.json"), 40).unwrap()
}

#[test]
fn profile_resolution_windows_matches_fixture() {
    let f = exe_fixture();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            resolve_exe_profile(
                c.input.exe.as_deref(),
                c.input.window_title.as_deref(),
                c.input.url.as_deref(),
            )
            .id != c.expected
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[test]
fn the_fixture_covers_every_listed_exe() {
    let f = exe_fixture();
    let covered: BTreeSet<String> = f
        .cases
        .iter()
        .filter_map(|c| c.input.exe.as_deref())
        .map(str::to_lowercase)
        .collect();
    let missing: Vec<&str> = exe_tables()
        .into_iter()
        .flat_map(|(_, names)| names.iter().copied())
        .filter(|n| !covered.contains(*n))
        .collect();
    assert!(missing.is_empty(), "exes without a case: {missing:?}");
}

#[test]
fn every_terminal_has_a_case_with_and_without_a_claude_title() {
    let f = exe_fixture();
    for (id, names) in exe_tables() {
        if id != "terminal" {
            continue;
        }
        for name in names {
            let titled = |claude: bool| {
                f.cases.iter().any(|c| {
                    c.input.exe.as_deref() == Some(name)
                        && c.input
                            .window_title
                            .as_deref()
                            .is_some_and(|t| t.to_lowercase().contains("claude") == claude)
                })
            };
            assert!(titled(true), "{name}: no claude-title case");
            assert!(titled(false), "{name}: no plain-title case");
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutCase {
    style: String,
    phase: String,
    hands_free: bool,
    hovering: bool,
    has_live: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ShapeOut {
    width: f64,
    height: f64,
    radius: f64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct WindowOut {
    width: f64,
    height: f64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LayoutOut {
    shape: ShapeOut,
    caption: bool,
    window: WindowOut,
}

type LayoutFixture = support::Envelope<LayoutCase, LayoutOut>;

fn layout_fixture() -> LayoutFixture {
    support::load_from(&windows_fixture("widget-layout-windows.json"), 24).unwrap()
}

fn phase_from(id: &str) -> Phase {
    match id {
        "idle" => Phase::Idle,
        "listening" => Phase::Listening,
        "processing" => Phase::Processing,
        "done" => Phase::Done,
        "failed" => Phase::Failed,
        other => panic!("unknown phase {other}"),
    }
}

fn layout_of(input: &LayoutCase) -> LayoutOut {
    let layout = layout_for(&LayoutInput {
        style: Style::from_id(&input.style).unwrap_or_else(|| panic!("style {}", input.style)),
        phase: phase_from(&input.phase),
        hands_free: input.hands_free,
        hovering: input.hovering,
        has_live: input.has_live,
    });
    LayoutOut {
        shape: ShapeOut {
            width: layout.shape.width,
            height: layout.shape.height,
            radius: layout.shape.radius,
        },
        caption: layout.caption,
        window: WindowOut {
            width: layout.window.width,
            height: layout.window.height,
        },
    }
}

#[test]
fn widget_layout_windows_matches_fixture() {
    let f = layout_fixture();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| layout_of(&c.input) != c.expected)
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[test]
fn the_layout_fixture_covers_every_style_and_state() {
    let f = layout_fixture();
    for style in ["flowPill", "captionBar", "island"] {
        for state in [
            "idle",
            "idle-hover",
            "ptt",
            "hands-free",
            "listening-live",
            "processing",
            "done",
            "failed",
        ] {
            let name = format!("{style}/{state}");
            assert!(f.cases.iter().any(|c| c.name == name), "no case {name}");
        }
    }
}

#[derive(Deserialize)]
struct KeyCase {
    vk: u32,
    flags: u32,
}

type KeyFixture = support::Envelope<KeyCase, Option<String>>;

fn key_fixture() -> KeyFixture {
    support::load_from(&windows_fixture("hotkey-candidates-windows.json"), 30).unwrap()
}

#[test]
fn hotkey_candidates_windows_matches_fixture() {
    let f = key_fixture();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            Hotkey::candidate(c.input.vk, c.input.flags).map(|h| h.id().to_string()) != c.expected
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}

#[test]
fn the_key_fixture_names_every_hotkey() {
    let f = key_fixture();
    for h in Hotkey::ALL {
        assert!(
            f.cases
                .iter()
                .any(|c| c.expected.as_deref() == Some(h.id())),
            "no case for {}",
            h.id()
        );
    }
}

#[test]
fn the_key_fixture_has_refused_keys() {
    let f = key_fixture();
    assert!(f.cases.iter().filter(|c| c.expected.is_none()).count() >= 10);
}

#[test]
fn the_picking_hook_filter_follows_the_key_fixture() {
    // What the picker may see is what the fixture calls a candidate, plus Esc.
    let f = key_fixture();
    let failing: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| {
            let want = c.expected.is_some() || c.input.vk == VK_ESCAPE;
            is_accepted(c.input.vk, c.input.flags, Hotkey::default(), true) != want
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(failing.is_empty(), "failing cases: {failing:?}");
}
