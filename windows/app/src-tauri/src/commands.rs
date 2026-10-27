//! The commands of the Settings and tour windows, and the payloads they return. Everything that
//! needs no window lives in pure functions here so it runs in tests on any host.

use crate::controller::{Msg, PickMsg};
use crate::history_store::HistoryStore;
use crate::polish;
use crate::settings_store;
use crate::ui;
use crate::widget::{self, DemoStep};
use crate::widget_host::WidgetHost;
use serde::Serialize;
use shepit_core::dictation::ModelStatus;
use shepit_core::keys::{Hotkey, PickResult};
use shepit_core::mic::MicOutcome;
use shepit_core::polish::discovery::fallback_provider;
use shepit_core::settings::Settings;
use shepit_core::widget::Style;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

/// The dictation key as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyPayload {
    pub id: String,
    pub label: String,
    pub is_default: bool,
}

/// The recognizer's state: "downloading", "loading", "ready" or "failed". `percent` is the
/// download as a whole number 0 to 100 (rounded like the tray tooltip) and null otherwise.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModelPayload {
    pub state: String,
    pub percent: Option<u32>,
}

/// `get_settings` and the `settings-changed` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    pub widget_style: String,
    pub glass_opacity: f64,
    pub fade_when_idle: bool,
    pub widget_visible: bool,
    pub hotkey: HotkeyPayload,
    pub model: ModelPayload,
    pub version: String,
    /// "claude", "codex" or "off".
    pub polishing: String,
    /// "haiku" or "sonnet".
    pub claude_model: String,
    /// Empty means found automatically.
    pub claude_executable: String,
    pub codex_executable: String,
    /// "raw", "clean" or "prompt".
    pub default_mode: String,
    pub fast_path_max_words: i64,
    pub prompt_mode_min_words: i64,
    pub speaker_notes: String,
    /// The daily look at GitHub (Settings > About).
    pub check_for_updates: bool,
    /// One term per entry, in stored order (Settings > Dictionary).
    pub dictionary: Vec<String>,
}

/// `detect_agents`: the resolved paths (null when not found) and the settings after the fallback
/// rule.
#[derive(Debug, Clone, Serialize)]
pub struct AgentsPayload {
    pub claude: Option<String>,
    pub codex: Option<String>,
    pub settings: SettingsPayload,
}

/// `check_microphone`: "ok", "silent", "denied", "noDevice" or "failed"; `detail` is the error
/// text for "failed" and null otherwise.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MicCheck {
    pub result: String,
    pub detail: Option<String>,
}

pub fn settings_payload(settings: &Settings, model: ModelStatus, version: &str) -> SettingsPayload {
    SettingsPayload {
        widget_style: settings.widget_style.id().to_string(),
        glass_opacity: settings.glass_opacity,
        fade_when_idle: settings.fade_when_idle,
        widget_visible: settings.widget_visible,
        hotkey: hotkey_payload(settings.hotkey),
        model: model_payload(model),
        version: version.to_string(),
        polishing: settings.polishing.id().to_string(),
        claude_model: settings.claude_model.id().to_string(),
        claude_executable: settings.claude_executable.clone(),
        codex_executable: settings.codex_executable.clone(),
        default_mode: settings.default_mode.as_str().to_string(),
        fast_path_max_words: settings.fast_path_max_words,
        prompt_mode_min_words: settings.prompt_mode_min_words,
        speaker_notes: settings.speaker_notes.clone(),
        check_for_updates: settings.check_for_updates,
        // Stub until the history work lands.
        dictionary: Vec::new(),
    }
}

/// One row of the History window. No window title, language, audio length or file name: the
/// window does not show them.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    pub created_at_ms: i64,
    pub app_name: Option<String>,
    /// The exe name.
    pub app_id: Option<String>,
    pub profile: String,
    pub mode: String,
    pub formatter_source: String,
    pub stt_ms: i64,
    pub llm_ms: i64,
    pub raw_text: String,
    pub formatted_text: String,
    pub status: String,
}

/// `history_recent`: the newest 200 rows, how many there are in all, and why there are none to
/// show (`error` is null when the store works).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistoryPayload {
    pub items: Vec<HistoryItem>,
    pub total: i64,
    pub error: Option<String>,
}

/// `store` is `None` when the database could not be opened at launch.
pub fn history_payload(store: Option<&HistoryStore>) -> HistoryPayload {
    let _ = store;
    HistoryPayload {
        items: Vec::new(),
        total: 0,
        error: None,
    }
}

/// Deletes every row and every recording. Rows go first; recordings are deleted even when the
/// rows could not be.
pub fn clear_history(
    store: Option<&HistoryStore>,
    recordings: &std::path::Path,
) -> Result<(), String> {
    let _ = (store, recordings);
    Ok(())
}

/// The settings with the agent choice moved off a missing agent (SharedControls.swift:81-89).
/// Only `polishing` can change.
pub fn fallback_settings(current: &Settings, claude_found: bool, codex_found: bool) -> Settings {
    Settings {
        polishing: fallback_provider(current.polishing, claude_found, codex_found),
        ..current.clone()
    }
}

pub fn hotkey_payload(hotkey: Hotkey) -> HotkeyPayload {
    HotkeyPayload {
        id: hotkey.id().to_string(),
        label: hotkey.label().to_string(),
        is_default: hotkey == Hotkey::default(),
    }
}

pub fn model_payload(status: ModelStatus) -> ModelPayload {
    let (state, percent) = match status {
        ModelStatus::Downloading(p) => ("downloading", Some((p * 100.0).round() as u32)),
        ModelStatus::Loading => ("loading", None),
        ModelStatus::Ready => ("ready", None),
        ModelStatus::Failed => ("failed", None),
    };
    ModelPayload {
        state: state.to_string(),
        percent,
    }
}

pub fn mic_check(outcome: MicOutcome) -> MicCheck {
    let detail = match &outcome {
        MicOutcome::Failed(text) => Some(text.clone()),
        _ => None,
    };
    MicCheck {
        result: outcome.id().to_string(),
        detail,
    }
}

/// The `hotkey-pick` event: `result` is "picked", "cancelled" or "timedOut"; `hotkey` is set only
/// for a pick.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PickPayload {
    pub result: String,
    pub hotkey: Option<HotkeyPayload>,
}

pub fn pick_payload(result: PickResult) -> PickPayload {
    let (name, hotkey) = match result {
        PickResult::Picked(h) => ("picked", Some(hotkey_payload(h))),
        PickResult::Cancelled => ("cancelled", None),
        PickResult::TimedOut => ("timedOut", None),
    };
    PickPayload {
        result: name.to_string(),
        hotkey,
    }
}

// The commands. Each is async on purpose: a sync command runs on the main thread, and the widget
// host asks the main thread for monitor data while it holds its lock.

type Model<'a> = State<'a, Arc<Mutex<ModelStatus>>>;

pub fn current_model(model: &Mutex<ModelStatus>) -> ModelStatus {
    *model.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn payload_of(app: &AppHandle, settings: &Settings) -> SettingsPayload {
    let model = current_model(&app.state::<Arc<Mutex<ModelStatus>>>());
    settings_payload(settings, model, &app.package_info().version.to_string())
}

#[tauri::command]
pub async fn get_settings(
    app: AppHandle,
    host: State<'_, WidgetHost>,
    model: Model<'_>,
) -> Result<SettingsPayload, String> {
    let version = app.package_info().version.to_string();
    Ok(settings_payload(
        &host.settings(),
        current_model(&model),
        &version,
    ))
}

/// The patch is strict: an unknown key, a wrong type or a bad id rejects all of it.
#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    patch: serde_json::Value,
) -> Result<SettingsPayload, String> {
    let (settings, _) = settings_store::apply(&app, |current| {
        shepit_core::settings::apply_patch(current, &patch)
    })?;
    Ok(payload_of(&app, &settings))
}

#[tauri::command]
pub async fn start_hotkey_pick(host: State<'_, WidgetHost>) -> Result<(), String> {
    host.send(Msg::Pick(PickMsg::Start))
}

#[tauri::command]
pub async fn cancel_hotkey_pick(host: State<'_, WidgetHost>) -> Result<(), String> {
    host.send(Msg::Pick(PickMsg::Cancel))
}

/// Opens the default input for a second, drops what it hears and reports how it went.
#[tauri::command]
pub async fn check_microphone(host: State<'_, WidgetHost>) -> Result<MicCheck, String> {
    // A dictation owns the microphone while it runs.
    if host.dictating() {
        return Ok(mic_check(MicOutcome::Failed(crate::mic_check::BUSY.into())));
    }
    let outcome = tauri::async_runtime::spawn_blocking(crate::mic_check::run)
        .await
        .map_err(|e| e.to_string())?;
    Ok(mic_check(outcome))
}

#[tauri::command]
pub async fn open_microphone_privacy() -> Result<(), String> {
    crate::shell::open_microphone_privacy()
}

#[tauri::command]
pub async fn reset_widget_position(host: State<'_, WidgetHost>) -> Result<(), String> {
    host.reset_position();
    Ok(())
}

#[tauri::command]
pub async fn widget_demo(
    host: State<'_, WidgetHost>,
    style: String,
) -> Result<Vec<DemoStep>, String> {
    let style = Style::from_id(&style).ok_or_else(|| format!("unknown style: {style}"))?;
    Ok(widget::demo_script(style, host.settings().hotkey))
}

/// Looks for Claude Code and Codex, moves the choice off a missing agent and tells the open
/// windows (`settings-changed`, through `apply`) when that changed anything. The settings go to the
/// polish dispatcher either way, so an agent installed after launch is picked up.
#[tauri::command]
pub async fn detect_agents(app: AppHandle) -> Result<AgentsPayload, String> {
    let current = app.state::<WidgetHost>().settings();
    let (claude, codex) =
        tauri::async_runtime::spawn_blocking(move || polish::find_agents(&current))
            .await
            .map_err(|e| e.to_string())?;
    let (settings, _) = settings_store::apply(&app, |cur| {
        Ok(fallback_settings(cur, claude.is_some(), codex.is_some()))
    })?;
    if let Some(polish) = app.try_state::<polish::PolishHandle>() {
        polish.settings(settings.clone());
    }
    Ok(AgentsPayload {
        claude,
        codex,
        settings: payload_of(&app, &settings),
    })
}

/// Marks the tour done and closes it.
#[tauri::command]
pub async fn finish_tour(app: AppHandle) -> Result<(), String> {
    ui::finish_tour(&app);
    Ok(())
}

#[tauri::command]
pub async fn show_tour(app: AppHandle) -> Result<(), String> {
    ui::open_tour(&app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::keys::Hotkey;
    use shepit_core::profile::Mode;
    use shepit_core::settings::{ClaudeModel, Polishing};
    use shepit_core::widget::Style;

    fn json(settings: &Settings, model: ModelStatus) -> serde_json::Value {
        serde_json::to_value(settings_payload(settings, model, "0.4.0")).unwrap()
    }

    #[test]
    fn the_settings_payload_has_the_agreed_keys() {
        let v = json(&Settings::default(), ModelStatus::Ready);
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "checkForUpdates",
                "claudeExecutable",
                "claudeModel",
                "codexExecutable",
                "defaultMode",
                "dictionary",
                "fadeWhenIdle",
                "fastPathMaxWords",
                "glassOpacity",
                "hotkey",
                "model",
                "polishing",
                "promptModeMinWords",
                "speakerNotes",
                "version",
                "widgetStyle",
                "widgetVisible"
            ]
        );
    }

    #[test]
    fn the_payload_carries_the_daily_update_switch() {
        let on = json(&Settings::default(), ModelStatus::Ready);
        assert_eq!(on["checkForUpdates"], true);
        let off = Settings {
            check_for_updates: false,
            ..Settings::default()
        };
        assert_eq!(json(&off, ModelStatus::Ready)["checkForUpdates"], false);
        // The skipped version and the last check time stay inside the app.
        assert!(on.get("skippedUpdate").is_none());
        assert!(on.get("lastUpdateCheck").is_none());
    }

    #[test]
    fn the_payload_carries_the_polishing_settings() {
        let s = Settings {
            polishing: Polishing::Codex,
            claude_model: ClaudeModel::Sonnet,
            claude_executable: r"C:\tools\claude.exe".into(),
            codex_executable: r"C:\tools\codex.cmd".into(),
            default_mode: Mode::Prompt,
            fast_path_max_words: 12,
            prompt_mode_min_words: 60,
            speaker_notes: "Notes.".into(),
            dictionary: vec!["Vercel".into()],
            ..Settings::default()
        };
        let v = json(&s, ModelStatus::Ready);
        assert_eq!(v["polishing"], "codex");
        assert_eq!(v["claudeModel"], "sonnet");
        assert_eq!(v["claudeExecutable"], r"C:\tools\claude.exe");
        assert_eq!(v["codexExecutable"], r"C:\tools\codex.cmd");
        assert_eq!(v["defaultMode"], "prompt");
        assert_eq!(v["fastPathMaxWords"], 12);
        assert_eq!(v["promptModeMinWords"], 60);
        assert_eq!(v["speakerNotes"], "Notes.");
        assert_eq!(v["dictionary"], serde_json::json!(["Vercel"]));
    }

    #[test]
    fn the_default_payload_polishes_with_claude_in_clean_mode() {
        let v = json(&Settings::default(), ModelStatus::Ready);
        assert_eq!(v["polishing"], "claude");
        assert_eq!(v["claudeModel"], "haiku");
        assert_eq!(v["claudeExecutable"], "");
        assert_eq!(v["defaultMode"], "clean");
    }

    fn with(polishing: Polishing) -> Settings {
        Settings {
            polishing,
            ..Settings::default()
        }
    }

    #[test]
    fn the_fallback_moves_a_missing_agent_to_the_one_that_is_found() {
        let cases = [
            (Polishing::Claude, false, true, Polishing::Codex),
            (Polishing::Codex, true, false, Polishing::Claude),
            (Polishing::Claude, false, false, Polishing::Off),
            (Polishing::Claude, true, false, Polishing::Claude),
            (Polishing::Off, true, true, Polishing::Off),
        ];
        for (selected, claude, codex, want) in cases {
            let after = fallback_settings(&with(selected), claude, codex);
            assert_eq!(after.polishing, want, "{selected:?} {claude} {codex}");
        }
    }

    #[test]
    fn a_fallback_that_changes_the_agent_reports_a_change_for_the_settings_changed_event() {
        let before = with(Polishing::Claude);
        let after = fallback_settings(&before, false, true);
        let c = shepit_core::settings::changes(&before, &after);
        assert!(c.polishing && c.any());
        let same = fallback_settings(&before, true, true);
        assert!(!shepit_core::settings::changes(&before, &same).any());
    }

    #[test]
    fn the_fallback_changes_nothing_but_the_choice() {
        let before = Settings {
            default_mode: Mode::Raw,
            speaker_notes: "keep".into(),
            ..with(Polishing::Claude)
        };
        let after = fallback_settings(&before, false, true);
        assert_eq!(
            after,
            Settings {
                polishing: Polishing::Codex,
                ..before
            }
        );
    }

    #[test]
    fn the_detect_agents_reply_has_claude_codex_and_settings() {
        let reply = AgentsPayload {
            claude: Some(r"C:\bin\claude.exe".into()),
            codex: None,
            settings: settings_payload(&Settings::default(), ModelStatus::Ready, "0.4.0"),
        };
        let v = serde_json::to_value(reply).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["claude", "codex", "settings"]);
        assert_eq!(v["claude"], r"C:\bin\claude.exe");
        assert!(v["codex"].is_null());
        assert_eq!(v["settings"]["polishing"], "claude");
    }

    #[test]
    fn the_payload_leaves_out_the_anchor_the_tour_flag_and_extras() {
        let mut s = Settings {
            widget_anchor: Some(shepit_core::widget::Point { x: 1, y: 2 }),
            onboarding_completed: true,
            ..Settings::default()
        };
        s.extra
            .insert("engine".into(), serde_json::Value::from("x"));
        let v = json(&s, ModelStatus::Ready);
        for key in ["widgetAnchor", "onboardingCompleted", "engine"] {
            assert!(v.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn the_payload_carries_the_look_and_the_version() {
        let s = Settings {
            widget_style: Style::CaptionBar,
            glass_opacity: 0.85,
            fade_when_idle: false,
            widget_visible: false,
            ..Settings::default()
        };
        let v = json(&s, ModelStatus::Ready);
        assert_eq!(v["widgetStyle"], "captionBar");
        assert_eq!(v["glassOpacity"], 0.85);
        assert_eq!(v["fadeWhenIdle"], false);
        assert_eq!(v["widgetVisible"], false);
        assert_eq!(v["version"], "0.4.0");
    }

    #[test]
    fn the_default_hotkey_is_marked_as_the_default() {
        let v = json(&Settings::default(), ModelStatus::Ready);
        assert_eq!(v["hotkey"]["id"], "rightCtrl");
        assert_eq!(v["hotkey"]["label"], "Right Ctrl");
        assert_eq!(v["hotkey"]["isDefault"], true);
        let keys: Vec<&String> = v["hotkey"].as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn another_hotkey_is_not_the_default() {
        let s = Settings {
            hotkey: Hotkey::RightAlt,
            ..Settings::default()
        };
        let v = json(&s, ModelStatus::Ready);
        assert_eq!(v["hotkey"]["id"], "rightAlt");
        assert_eq!(v["hotkey"]["label"], "Right Alt");
        assert_eq!(v["hotkey"]["isDefault"], false);
    }

    #[test]
    fn the_payload_embeds_the_model_state() {
        let v = json(&Settings::default(), ModelStatus::Downloading(0.42));
        assert_eq!(v["model"]["state"], "downloading");
        assert_eq!(v["model"]["percent"], 42);
    }

    #[test]
    fn each_model_status_maps_to_a_state_name() {
        let cases = [
            (ModelStatus::Loading, "loading", None),
            (ModelStatus::Ready, "ready", None),
            (ModelStatus::Failed, "failed", None),
            (ModelStatus::Downloading(0.0), "downloading", Some(0)),
            (ModelStatus::Downloading(0.424), "downloading", Some(42)),
            (ModelStatus::Downloading(0.425), "downloading", Some(43)),
            (ModelStatus::Downloading(1.0), "downloading", Some(100)),
        ];
        for (status, state, percent) in cases {
            let p = model_payload(status);
            assert_eq!(p.state, state, "{status:?}");
            assert_eq!(p.percent, percent, "{status:?}");
        }
    }

    #[test]
    fn the_model_payload_always_has_both_keys() {
        let v = serde_json::to_value(model_payload(ModelStatus::Ready)).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["percent", "state"]);
        assert!(v["percent"].is_null());
    }

    #[test]
    fn a_mic_check_names_the_result_and_only_a_failure_has_detail() {
        let cases = [
            (MicOutcome::Ok, "ok", None),
            (MicOutcome::Silent, "silent", None),
            (MicOutcome::Denied, "denied", None),
            (MicOutcome::NoDevice, "noDevice", None),
            (
                MicOutcome::Failed("stream broke".into()),
                "failed",
                Some("stream broke".to_string()),
            ),
        ];
        for (outcome, result, detail) in cases {
            let c = mic_check(outcome.clone());
            assert_eq!(c.result, result, "{outcome:?}");
            assert_eq!(c.detail, detail, "{outcome:?}");
        }
    }

    #[test]
    fn the_mic_check_json_has_result_and_detail() {
        let v = serde_json::to_value(mic_check(MicOutcome::Ok)).unwrap();
        assert_eq!(v["result"], "ok");
        assert!(v["detail"].is_null());
        assert_eq!(v.as_object().unwrap().len(), 2);
    }

    #[test]
    fn the_payload_dictionary_keeps_order_and_duplicates() {
        let none = json(&Settings::default(), ModelStatus::Ready);
        assert_eq!(none["dictionary"], serde_json::json!([]));
        let s = Settings {
            dictionary: vec!["B".into(), "a".into(), "B".into()],
            ..Settings::default()
        };
        let v = json(&s, ModelStatus::Ready);
        assert_eq!(v["dictionary"], serde_json::json!(["B", "a", "B"]));
    }

    // History.

    use crate::history_store::HistoryStore;
    use shepit_core::dictation::RecordDraft;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("shepit-commands-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn draft(raw_text: &str) -> RecordDraft {
        RecordDraft {
            app_id: Some("code.exe".into()),
            app_name: Some("Visual Studio Code".into()),
            window_title: Some("secret title".into()),
            profile: "generic".into(),
            mode: "clean".into(),
            engine: "parakeet-v3".into(),
            raw_text: raw_text.into(),
            formatted_text: format!("{raw_text}."),
            language: Some("en".into()),
            audio_seconds: 2.5,
            stt_ms: 350,
            llm_ms: 420,
            formatter_source: "claude".into(),
            insert_method: "paste".into(),
            status: "inserted".into(),
        }
    }

    fn keys(v: &serde_json::Value) -> Vec<&str> {
        let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        k.sort();
        k
    }

    #[test]
    fn the_history_payload_has_items_total_and_error() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        store
            .save(1_700_000_000_123, &draft("hello"), Some("a.wav"))
            .unwrap();
        let v = serde_json::to_value(history_payload(Some(&store))).unwrap();
        assert_eq!(keys(&v), ["error", "items", "total"]);
        assert_eq!(v["total"], 1);
        assert!(v["error"].is_null());
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_history_item_carries_exactly_the_agreed_keys() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        let id = store
            .save(1_700_000_000_123, &draft("hello"), Some("a.wav"))
            .unwrap();
        let v = serde_json::to_value(history_payload(Some(&store))).unwrap();
        let item = &v["items"][0];
        assert_eq!(
            keys(item),
            [
                "appId",
                "appName",
                "createdAtMs",
                "formattedText",
                "formatterSource",
                "id",
                "llmMs",
                "mode",
                "profile",
                "rawText",
                "status",
                "sttMs"
            ]
        );
        assert_eq!(item["id"], id);
        assert_eq!(item["createdAtMs"], 1_700_000_000_123_i64);
        assert_eq!(item["appName"], "Visual Studio Code");
        assert_eq!(item["appId"], "code.exe");
        assert_eq!(item["profile"], "generic");
        assert_eq!(item["mode"], "clean");
        assert_eq!(item["formatterSource"], "claude");
        assert_eq!(
            (item["sttMs"].as_i64(), item["llmMs"].as_i64()),
            (Some(350), Some(420))
        );
        assert_eq!(item["rawText"], "hello");
        assert_eq!(item["formattedText"], "hello.");
        assert_eq!(item["status"], "inserted");
    }

    #[test]
    fn the_history_payload_is_the_newest_two_hundred_and_the_total() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        for i in 0..205 {
            store
                .save(1_000 + i, &draft(&format!("row {i}")), None)
                .unwrap();
        }
        let p = history_payload(Some(&store));
        assert_eq!((p.items.len(), p.total, p.error), (200, 205, None));
        assert_eq!(p.items[0].raw_text, "row 204");
    }

    #[test]
    fn an_unavailable_store_says_so_instead_of_showing_an_empty_history() {
        let p = history_payload(None);
        assert!(p.items.is_empty());
        assert_eq!(p.total, 0);
        assert!(p.error.is_some_and(|e| !e.is_empty()));
    }

    #[test]
    fn clearing_deletes_every_row_and_every_recording() {
        let t = TempDir::new();
        let store = HistoryStore::open(&t.0).unwrap();
        let recordings = crate::recordings::dir_in(&t.0);
        fs::create_dir_all(&recordings).unwrap();
        for name in ["a.wav", "b.wav"] {
            fs::write(recordings.join(name), b"x").unwrap();
        }
        fs::write(recordings.join("keep.txt"), b"x").unwrap();
        for i in 0..3 {
            store.save(1_000 + i, &draft("x"), Some("a.wav")).unwrap();
        }
        clear_history(Some(&store), &recordings).unwrap();
        assert_eq!(store.count().unwrap(), 0);
        assert!(!recordings.join("a.wav").exists() && !recordings.join("b.wav").exists());
        assert!(recordings.join("keep.txt").exists());
    }

    #[test]
    fn clearing_deletes_the_recordings_even_without_a_store() {
        let t = TempDir::new();
        let recordings = crate::recordings::dir_in(&t.0);
        fs::create_dir_all(&recordings).unwrap();
        fs::write(recordings.join("a.wav"), b"x").unwrap();
        let _ = clear_history(None, &recordings);
        assert!(!recordings.join("a.wav").exists());
    }
}
