//! Windows updater wiring (docs/spec/updates-install.md). STUBS: the tests below define the
//! behaviour; the bodies are `todo!()` until the implementer fills them in.
#![allow(dead_code, unused_variables)]

use serde::Serialize;
use shepit_core::update::{CheckOutcome, UpdateState};
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Config, UpdaterBuilder};

/// A check that has not answered after this long is an error (ReleaseFeed.swift:61).
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

/// The plugin asks this for every release it finds. Same rule as the other platform.
pub fn accept_release(installed: &str, remote: &str) -> bool {
    todo!()
}

/// The updater plugin the app registers. The key and the endpoint come from tauri.conf.json only.
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R, Config> {
    todo!()
}

/// The builder for one check of the installed app.
pub fn checker<R: Runtime>(app: &AppHandle<R>) -> UpdaterBuilder {
    todo!()
}

/// What `get_update_status` and the `update-status` event carry.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    /// idle, checking, upToDate, available, downloading, installing, failed or installFailed.
    pub state: String,
    pub label: String,
    /// The error text for `failed` and `installFailed`, else null.
    pub message: Option<String>,
    pub last_check: Option<u64>,
}

pub fn status_payload(
    state: &UpdateState,
    total: Option<u64>,
    last_check: Option<u64>,
) -> StatusPayload {
    todo!()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Progress {
    /// None draws an indeterminate bar.
    pub fraction: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardButton {
    pub id: String,
    pub label: String,
    /// "quiet" or "primary".
    pub kind: String,
    /// Sits at the left edge, the rest at the right.
    pub lead: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardPayload {
    pub title: String,
    pub detail: Option<String>,
    pub progress: Option<Progress>,
    pub link: Option<CardButton>,
    pub buttons: Vec<CardButton>,
}

/// `installed` is the running version; `total` the download size in bytes when known.
pub fn card_for(state: &UpdateState, installed: &str, total: Option<u64>) -> CardPayload {
    todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Install,
    Skip,
    Dismiss,
    WhatChanged,
    ReleasePage,
}

impl CardAction {
    /// `install`, `skip`, `dismiss`, `whatChanged` or `releasePage`; exact and case-sensitive.
    pub fn parse(id: &str) -> Option<CardAction> {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The click does nothing in this state.
    Ignore,
    Install(String),
    /// Store the version as skipped and close the card.
    Skip(String),
    /// `version` None means the latest releases page.
    OpenPage {
        version: Option<String>,
        close: bool,
    },
    Close,
}

pub fn action_effect(state: &UpdateState, action: CardAction) -> Effect {
    todo!()
}

/// A check result opens the card only when it found a release the user should see.
pub fn opens_card(outcome: &CheckOutcome) -> bool {
    todo!()
}

#[tauri::command]
pub async fn get_update_status() -> Result<StatusPayload, String> {
    todo!()
}

#[tauri::command]
pub async fn check_updates_now() -> Result<(), String> {
    todo!()
}

#[tauri::command]
pub async fn show_update_card() -> Result<(), String> {
    todo!()
}

#[tauri::command]
pub async fn update_card_ready() -> Result<CardPayload, String> {
    todo!()
}

#[tauri::command]
pub async fn update_action(action: String) -> Result<(), String> {
    todo!()
}

#[tauri::command]
pub async fn update_resize(height: f64) -> Result<(), String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use shepit_core::update::{on_check_result, CheckResult};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::App;

    const NOW: u64 = 1_800_000_000;

    fn v(s: &str) -> String {
        s.to_string()
    }

    // accept_release

    #[test]
    fn a_newer_release_is_accepted() {
        assert!(accept_release("0.3.1", "0.4.0"));
        assert!(accept_release("0.3.1", "0.3.2"));
        assert!(accept_release("0.9.0", "0.10.0"));
    }

    #[test]
    fn the_same_version_is_not_accepted() {
        assert!(!accept_release("0.3.1", "0.3.1"));
    }

    #[test]
    fn an_older_release_is_not_accepted() {
        assert!(!accept_release("0.3.1", "0.3.0"));
        assert!(!accept_release("0.3.1", "0.2.9"));
    }

    #[test]
    fn a_v_prefix_does_not_change_the_order() {
        assert!(accept_release("0.3.1", "v0.4.0"));
        assert!(!accept_release("v0.3.1", "0.3.1"));
        assert!(!accept_release("0.3.1", "v0.3.0"));
    }

    #[test]
    fn the_check_times_out_after_20_seconds() {
        assert_eq!(CHECK_TIMEOUT, Duration::from_secs(20));
    }

    // status_payload

    fn status(state: &UpdateState, total: Option<u64>, last: Option<u64>) -> Value {
        serde_json::to_value(status_payload(state, total, last)).unwrap()
    }

    #[test]
    fn the_status_payload_has_the_agreed_keys() {
        let s = status(&UpdateState::Idle, None, None);
        let mut keys: Vec<&str> = s.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["label", "lastCheck", "message", "state"]);
    }

    #[test]
    fn idle_and_up_to_date_read_up_to_date() {
        for (state, id) in [
            (UpdateState::Idle, "idle"),
            (UpdateState::UpToDate, "upToDate"),
        ] {
            let s = status(&state, None, Some(NOW));
            assert_eq!(s["state"], id);
            assert_eq!(s["label"], "Up to date");
            assert_eq!(s["message"], Value::Null);
            assert_eq!(s["lastCheck"], NOW);
        }
    }

    #[test]
    fn checking_reads_checking_github() {
        let s = status(&UpdateState::Checking, None, None);
        assert_eq!(s["state"], "checking");
        assert_eq!(s["label"], "Checking GitHub...");
        assert_eq!(s["lastCheck"], Value::Null);
    }

    #[test]
    fn an_available_release_names_its_version() {
        let s = status(&UpdateState::Available(v("0.4.0")), None, None);
        assert_eq!(s["state"], "available");
        assert_eq!(s["label"], "Shepit 0.4.0 is out");
    }

    #[test]
    fn a_download_shows_the_percent_when_the_size_is_known() {
        let s = status(
            &UpdateState::Downloading(v("0.4.0"), 0.256),
            Some(48_000_000),
            None,
        );
        assert_eq!(s["state"], "downloading");
        assert_eq!(s["label"], "Downloading 0.4.0: 25%");
    }

    #[test]
    fn a_download_without_a_size_shows_no_percent() {
        let s = status(&UpdateState::Downloading(v("0.4.0"), 0.0), None, None);
        assert_eq!(s["label"], "Downloading 0.4.0...");
    }

    #[test]
    fn installing_names_its_version() {
        let s = status(&UpdateState::Installing(v("0.4.0")), None, None);
        assert_eq!(s["state"], "installing");
        assert_eq!(s["label"], "Installing 0.4.0...");
    }

    #[test]
    fn failures_carry_their_message() {
        let s = status(&UpdateState::Failed(v("offline")), None, None);
        assert_eq!(s["state"], "failed");
        assert_eq!(s["label"], "Last check failed");
        assert_eq!(s["message"], "offline");
        let s = status(
            &UpdateState::InstallFailed(v("0.4.0"), v("disk full")),
            None,
            None,
        );
        assert_eq!(s["state"], "installFailed");
        assert_eq!(s["label"], "The update did not install");
        assert_eq!(s["message"], "disk full");
    }

    // The card

    #[test]
    fn the_cards_equal_the_golden_file() {
        let golden: Value =
            serde_json::from_str(include_str!("../tests/fixtures/update-cards.json")).unwrap();
        let installed = golden["installed"].as_str().unwrap();
        let cases = golden["cases"].as_object().unwrap();
        assert_eq!(cases.len(), 8, "{:?}", cases.keys().collect::<Vec<_>>());
        for (name, case) in cases {
            let input = &case["input"];
            let text = |key: &str| input[key].as_str().unwrap_or_default().to_string();
            let state = match input["state"].as_str().unwrap() {
                "checking" => UpdateState::Checking,
                "upToDate" => UpdateState::UpToDate,
                "available" => UpdateState::Available(text("version")),
                "downloading" => {
                    UpdateState::Downloading(text("version"), input["fraction"].as_f64().unwrap())
                }
                "installing" => UpdateState::Installing(text("version")),
                "failed" => UpdateState::Failed(text("message")),
                "installFailed" => UpdateState::InstallFailed(text("version"), text("message")),
                other => panic!("{name}: unknown state {other}"),
            };
            let card = card_for(&state, installed, input["total"].as_u64());
            assert_eq!(serde_json::to_value(card).unwrap(), case["card"], "{name}");
        }
    }

    #[test]
    fn an_idle_state_shows_the_checking_card() {
        let idle = card_for(&UpdateState::Idle, "0.3.1", None);
        assert_eq!(idle, card_for(&UpdateState::Checking, "0.3.1", None));
    }

    #[test]
    fn actions_parse_exactly() {
        for (id, action) in [
            ("install", CardAction::Install),
            ("skip", CardAction::Skip),
            ("dismiss", CardAction::Dismiss),
            ("whatChanged", CardAction::WhatChanged),
            ("releasePage", CardAction::ReleasePage),
        ] {
            assert_eq!(CardAction::parse(id), Some(action), "{id}");
        }
    }

    #[test]
    fn actions_are_case_sensitive_and_exact() {
        for bad in [
            "Install",
            "INSTALL",
            "install ",
            " skip",
            "whatchanged",
            "release_page",
            "",
            "later",
        ] {
            assert_eq!(CardAction::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn install_and_skip_work_only_while_a_release_is_offered() {
        let offered = UpdateState::Available(v("0.4.0"));
        assert_eq!(
            action_effect(&offered, CardAction::Install),
            Effect::Install(v("0.4.0"))
        );
        assert_eq!(
            action_effect(&offered, CardAction::Skip),
            Effect::Skip(v("0.4.0"))
        );
        let others = [
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::UpToDate,
            UpdateState::Downloading(v("0.4.0"), 0.5),
            UpdateState::Installing(v("0.4.0")),
            UpdateState::Failed(v("x")),
            UpdateState::InstallFailed(v("0.4.0"), v("x")),
        ];
        for state in &others {
            assert_eq!(
                action_effect(state, CardAction::Install),
                Effect::Ignore,
                "{state:?}"
            );
            assert_eq!(
                action_effect(state, CardAction::Skip),
                Effect::Ignore,
                "{state:?}"
            );
        }
    }

    #[test]
    fn dismiss_closes_the_card_in_every_state() {
        for state in [
            UpdateState::Checking,
            UpdateState::UpToDate,
            UpdateState::Available(v("0.4.0")),
            UpdateState::Failed(v("x")),
            UpdateState::InstallFailed(v("0.4.0"), v("x")),
        ] {
            assert_eq!(
                action_effect(&state, CardAction::Dismiss),
                Effect::Close,
                "{state:?}"
            );
        }
    }

    #[test]
    fn what_changed_opens_the_page_and_keeps_the_card() {
        assert_eq!(
            action_effect(&UpdateState::Available(v("0.4.0")), CardAction::WhatChanged),
            Effect::OpenPage {
                version: Some(v("0.4.0")),
                close: false
            }
        );
    }

    #[test]
    fn the_release_page_button_opens_the_page_and_closes_the_card() {
        assert_eq!(
            action_effect(&UpdateState::Failed(v("x")), CardAction::ReleasePage),
            Effect::OpenPage {
                version: None,
                close: true
            }
        );
        assert_eq!(
            action_effect(
                &UpdateState::InstallFailed(v("0.4.0"), v("x")),
                CardAction::ReleasePage
            ),
            Effect::OpenPage {
                version: Some(v("0.4.0")),
                close: true
            }
        );
    }

    // opens_card

    #[test]
    fn a_found_release_opens_the_card() {
        let out = on_check_result(CheckResult::Newer(v("0.4.0")), false, "", NOW);
        assert!(opens_card(&out));
        let out = on_check_result(CheckResult::Newer(v("0.4.0")), true, "0.4.0", NOW);
        assert!(opens_card(&out));
    }

    #[test]
    fn a_skipped_release_found_in_the_background_does_not_open_the_card() {
        let out = on_check_result(CheckResult::Newer(v("0.4.0")), false, "0.4.0", NOW);
        assert!(!opens_card(&out));
    }

    #[test]
    fn up_to_date_and_errors_do_not_open_the_card_from_a_check() {
        // The tray opens its card before it checks; Settings > About never opens one for these.
        for result in [CheckResult::UpToDate, CheckResult::Error(v("offline"))] {
            let out = on_check_result(result, true, "", NOW);
            assert!(!opens_card(&out));
        }
    }

    // Functional trust tests. A mock app hosts the real plugin; a server on 127.0.0.1 plays GitHub.

    const KEY_C: &str = include_str!("../../../release-check/tests/fixtures/key-c.pub");
    const PAYLOAD: &[u8] = include_bytes!("../../../release-check/tests/fixtures/payload.bin");
    const FEED: &str = include_str!("../tests/fixtures/latest.json");

    /// Serves /latest.json (the fixture, with `version` replaced) and /payload.bin. Returns the
    /// base URL. The thread ends with the test process.
    fn serve(version: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let feed = FEED.replace("{base}", &base).replace(
            "\"version\": \"0.3.1\"",
            &format!("\"version\": \"{version}\""),
        );
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let (status, kind, body): (&str, &str, Vec<u8>) = match path {
                    "/latest.json" => ("200 OK", "application/json", feed.clone().into_bytes()),
                    "/payload.bin" => ("200 OK", "application/octet-stream", PAYLOAD.to_vec()),
                    _ => ("404 Not Found", "text/plain", b"not found".to_vec()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        base
    }

    /// A mock app (version 0.1.0) whose updater config is the real `plugins.updater` of
    /// tauri.conf.json.
    fn host(plugin: tauri::plugin::TauriPlugin<MockRuntime, Config>) -> App<MockRuntime> {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut context = mock_context(noop_assets());
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), conf["plugins"]["updater"].clone());
        mock_builder()
            .plugin(plugin)
            .build(context)
            .expect("mock app")
    }

    /// The app's checker, pointed at the local server. Only these overrides are test-only.
    fn local_checker(app: &App<MockRuntime>, base: &str) -> tauri_plugin_updater::Updater {
        checker(app.handle())
            .endpoints(vec![format!("{base}/latest.json").parse().unwrap()])
            .unwrap()
            .target("windows-x86_64")
            .no_proxy()
            .build()
            .unwrap()
    }

    /// Chunk bytes seen, whether the finish hook ran, and the download result.
    struct Downloaded {
        bytes: usize,
        finished: bool,
        result: Result<Vec<u8>, tauri_plugin_updater::Error>,
    }

    fn download_from(app: &App<MockRuntime>, base: &str) -> Downloaded {
        tauri::async_runtime::block_on(async {
            let update = local_checker(app, base)
                .check()
                .await
                .expect("the check works")
                .expect("the feed offers 0.3.1");
            let mut bytes = 0;
            let mut finished = false;
            let result = update.download(|n, _| bytes += n, || finished = true).await;
            Downloaded {
                bytes,
                finished,
                result,
            }
        })
    }

    #[test]
    fn the_harness_accepts_a_release_signed_with_the_key_it_trusts() {
        // Proves the server, the fixtures and the mock app work, so a refusal in the next test
        // is about the key and nothing else.
        let base = serve("0.3.1");
        let plugin = tauri_plugin_updater::Builder::new()
            .pubkey(KEY_C.trim())
            .default_version_comparator(|installed, remote| {
                accept_release(&installed.to_string(), &remote.version.to_string())
            })
            .build::<MockRuntime>();
        let got = download_from(&host(plugin), &base);
        assert_eq!(got.bytes, PAYLOAD.len());
        assert!(got.finished);
        assert_eq!(
            got.result.expect("a release signed with the trusted key"),
            PAYLOAD
        );
    }

    #[test]
    fn the_apps_updater_refuses_a_release_signed_with_another_key() {
        // Same feed, same bytes, signed with key C; the app trusts only the key in tauri.conf.json.
        let base = serve("0.3.1");
        let got = download_from(&host(plugin::<MockRuntime>()), &base);
        assert_eq!(got.bytes, PAYLOAD.len(), "the whole payload was downloaded");
        assert!(
            got.finished,
            "the check ran after the download, at the verify step"
        );
        assert!(
            got.result.is_err(),
            "a release signed with another key was accepted"
        );
    }

    #[test]
    fn the_same_and_older_versions_are_not_offered() {
        // The mock app is 0.1.0.
        for version in ["0.1.0", "0.0.9"] {
            let base = serve(version);
            let app = host(plugin::<MockRuntime>());
            let found = tauri::async_runtime::block_on(local_checker(&app, &base).check());
            assert!(
                matches!(found, Ok(None)),
                "{version}: {:?}",
                found.map(|u| u.map(|u| u.version))
            );
        }
    }

    #[test]
    fn a_newer_version_is_offered_by_the_app_updater() {
        let base = serve("0.3.1");
        let app = host(plugin::<MockRuntime>());
        let found = tauri::async_runtime::block_on(local_checker(&app, &base).check())
            .expect("the check works")
            .expect("0.3.1 is newer than 0.1.0");
        assert_eq!(found.version, "0.3.1");
    }
}
