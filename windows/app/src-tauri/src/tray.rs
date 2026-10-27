use shepit_core::dictation::ModelStatus;
use shepit_core::update::UpdateState;

#[derive(Debug, PartialEq, Eq)]
pub enum MenuAction {
    Quit,
    OpenSettings,
    OpenHistory,
    ToggleWidget,
    ResetWidget,
    CheckUpdates,
    None,
}

/// The "Show widget" check item, kept so a change made in Settings can update it.
pub struct ShowWidgetItem(pub tauri::menu::CheckMenuItem<tauri::Wry>);

/// The "Check for updates…" item, kept so the updater can rename it to "Update to X…".
pub struct UpdateItem(pub tauri::menu::MenuItem<tauri::Wry>);

/// The History item sits right after Settings (StatusItemController.swift:98-100).
pub const HISTORY_LABEL: &str = "History\u{2026}";

/// The ids of the menu items in order; separators are not listed. `main.rs` builds the menu in
/// this order and the tests pin it.
#[cfg_attr(not(test), allow(dead_code))]
pub const MENU_IDS: &[&str] = &[
    "settings",
    "history",
    "check-updates",
    "toggle-widget",
    "reset-widget",
    "quit",
];

pub fn action_for(id: &str) -> MenuAction {
    match id {
        "quit" => MenuAction::Quit,
        "settings" => MenuAction::OpenSettings,
        "history" => MenuAction::OpenHistory,
        "toggle-widget" => MenuAction::ToggleWidget,
        "reset-widget" => MenuAction::ResetWidget,
        "check-updates" => MenuAction::CheckUpdates,
        _ => MenuAction::None,
    }
}

/// The tray item next to Settings: offers the release when one is found, else starts a check.
pub fn update_label(state: &UpdateState) -> String {
    match state {
        UpdateState::Available(version) => format!("Update to {version}\u{2026}"),
        _ => "Check for updates\u{2026}".to_string(),
    }
}

/// Tray tooltip for the model state: the download percent while it runs, else the app name.
pub fn tooltip_for(status: ModelStatus) -> String {
    match status {
        ModelStatus::Downloading(p) => format!("Downloading model {}%", (p * 100.0).round() as u32),
        _ => "Shepit".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_shows_the_download_percent() {
        assert_eq!(
            tooltip_for(ModelStatus::Downloading(0.42)),
            "Downloading model 42%"
        );
        assert_eq!(
            tooltip_for(ModelStatus::Downloading(0.0)),
            "Downloading model 0%"
        );
    }

    #[test]
    fn tooltip_is_the_name_otherwise() {
        for s in [
            ModelStatus::Loading,
            ModelStatus::Ready,
            ModelStatus::Failed,
        ] {
            assert_eq!(tooltip_for(s), "Shepit");
        }
    }

    #[test]
    fn quit_id_quits() {
        assert_eq!(action_for("quit"), MenuAction::Quit);
    }

    #[test]
    fn settings_id_opens_settings() {
        assert_eq!(action_for("settings"), MenuAction::OpenSettings);
    }

    #[test]
    fn settings_id_is_case_sensitive_and_exact() {
        assert_eq!(action_for("Settings"), MenuAction::None);
        assert_eq!(action_for("settings "), MenuAction::None);
    }

    #[test]
    fn other_id_is_none() {
        assert_eq!(action_for("bogus"), MenuAction::None);
    }

    #[test]
    fn empty_id_is_none() {
        assert_eq!(action_for(""), MenuAction::None);
    }

    #[test]
    fn toggle_widget_id_toggles_the_widget() {
        assert_eq!(action_for("toggle-widget"), MenuAction::ToggleWidget);
    }

    #[test]
    fn reset_widget_id_resets_the_position() {
        assert_eq!(action_for("reset-widget"), MenuAction::ResetWidget);
    }

    #[test]
    fn widget_ids_are_case_sensitive_and_exact() {
        assert_eq!(action_for("Toggle-Widget"), MenuAction::None);
        assert_eq!(action_for("reset_widget"), MenuAction::None);
    }

    #[test]
    fn check_updates_id_checks_for_updates() {
        assert_eq!(action_for("check-updates"), MenuAction::CheckUpdates);
    }

    #[test]
    fn check_updates_id_is_case_sensitive_and_exact() {
        assert_eq!(action_for("Check-Updates"), MenuAction::None);
        assert_eq!(action_for("check_updates"), MenuAction::None);
        assert_eq!(action_for("check-updates "), MenuAction::None);
    }

    #[test]
    fn the_update_item_asks_to_check_until_a_release_is_found() {
        for state in [
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::UpToDate,
            UpdateState::Failed("offline".into()),
        ] {
            assert_eq!(update_label(&state), "Check for updates…", "{state:?}");
        }
    }

    #[test]
    fn the_update_item_names_the_release_that_is_out() {
        assert_eq!(
            update_label(&UpdateState::Available("0.4.0".into())),
            "Update to 0.4.0…"
        );
    }

    #[test]
    fn ids_are_case_sensitive() {
        assert_eq!(action_for("QUIT"), MenuAction::None);
    }

    #[test]
    fn history_id_opens_history() {
        assert_eq!(action_for("history"), MenuAction::OpenHistory);
        assert_eq!(action_for("History"), MenuAction::None);
        assert_eq!(action_for("history "), MenuAction::None);
    }

    #[test]
    fn the_history_item_is_labelled_with_an_ellipsis_character() {
        assert_eq!(HISTORY_LABEL, "History\u{2026}");
        assert!(!HISTORY_LABEL.ends_with("..."));
    }

    #[test]
    fn the_history_item_comes_right_after_settings() {
        let settings = MENU_IDS.iter().position(|id| *id == "settings").unwrap();
        let history = MENU_IDS.iter().position(|id| *id == "history");
        assert_eq!(history, Some(settings + 1), "{MENU_IDS:?}");
    }

    #[test]
    fn every_menu_id_has_an_action() {
        for id in MENU_IDS {
            assert_ne!(action_for(id), MenuAction::None, "{id}");
        }
    }
}
