//! Fixtures that only the Windows core runs (no Swift runner exists for them, so they stay out of
//! `shared/fixtures`).
mod support;

use serde::Deserialize;
use shepit_core::profile::{exe_tables, resolve_exe_profile};
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
