// The app commands are listed here so each window gets only its own (capabilities/*.json). Without a
// manifest every window could call every command. Widget commands first, then the Settings and tour ones, the history ones, then the update ones.
const COMMANDS: &[&str] = &[
    "widget_ready",
    "widget_hover",
    "widget_pointer",
    "get_settings",
    "update_settings",
    "start_hotkey_pick",
    "cancel_hotkey_pick",
    "check_microphone",
    "open_microphone_privacy",
    "reset_widget_position",
    "widget_demo",
    "finish_tour",
    "show_tour",
    "detect_agents",
    "history_recent",
    "history_clear",
    "open_recordings_folder",
    "get_update_status",
    "check_updates_now",
    "show_update_card",
    "update_card_ready",
    "update_action",
    "update_resize",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
