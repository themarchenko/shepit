//! Reads and writes `settings.json` in the app config directory. Writes go to a temp file that is
//! renamed over the real one. A file that failed to parse is copied to `settings.json.bak` before
//! the first write replaces it.

use crate::controller::Msg;
use crate::widget_host::WidgetHost;
use crate::{commands, tray, ui};
use shepit_core::settings::{Changes, Settings};
use std::fs;
use std::io;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const FILE: &str = "settings.json";
const TMP: &str = "settings.json.tmp";
const BAK: &str = "settings.json.bak";

pub struct SettingsStore {
    dir: PathBuf,
    corrupt: bool,
}

impl SettingsStore {
    /// `dir` is the config directory; it may not exist yet.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            corrupt: false,
        }
    }

    /// The defaults when the file is missing or corrupt. Remembers a corrupt file for `save`.
    pub fn load(&mut self) -> Settings {
        // An unreadable file (permissions, not UTF-8) is treated like a corrupt one.
        let text = match fs::read_to_string(self.dir.join(FILE)) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(_) => {
                self.corrupt = true;
                return Settings::default();
            }
        };
        let loaded = Settings::parse(&text);
        self.corrupt = loaded.corrupt;
        loaded.settings
    }

    /// Creates the directory if needed, backs up a corrupt original once, then writes atomically.
    pub fn save(&mut self, settings: &Settings) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let file = self.dir.join(FILE);
        if self.corrupt {
            // Copy before the rename replaces the only record of what the user had. If the file is
            // gone by now there is nothing to keep, and failing would block every later save.
            match fs::copy(&file, self.dir.join(BAK)) {
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
            self.corrupt = false;
        }
        let tmp = self.dir.join(TMP);
        fs::write(&tmp, settings.to_json())?;
        fs::rename(&tmp, &file).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}

/// The one write path for the settings the UI can change. `edit` makes the new settings from the
/// current ones (an `Err` changes nothing). Returns the settings now in force and what changed. Then it saves, tells the widget (look and key label),
/// sets the tray "Show widget" check, tells the open Settings and tour windows with
/// `settings-changed`, and sends the controller `Msg::Hotkey` reliably (see `WidgetHost::send_reliably`).
pub fn apply(
    app: &AppHandle,
    edit: impl FnOnce(&Settings) -> Result<Settings, String>,
) -> Result<(Settings, Changes), String> {
    apply_inner(app, edit, true)
}

/// `apply` for the controller thread, which switches its own key directly: sending to its own
/// bounded channel would block it forever once the channel is full.
pub fn apply_on_controller(
    app: &AppHandle,
    edit: impl FnOnce(&Settings) -> Result<Settings, String>,
) -> Result<(Settings, Changes), String> {
    apply_inner(app, edit, false)
}

fn apply_inner(
    app: &AppHandle,
    edit: impl FnOnce(&Settings) -> Result<Settings, String>,
    tell_controller: bool,
) -> Result<(Settings, Changes), String> {
    let host = app.state::<WidgetHost>();
    let (settings, changes) = host.update(edit)?;
    if changes.visible {
        if let Some(item) = app.try_state::<tray::ShowWidgetItem>() {
            let _ = item.0.set_checked(settings.widget_visible);
        }
    }
    if changes.any() {
        ui::emit_to_windows(
            app,
            "settings-changed",
            commands::payload_of(app, &settings),
        );
    }
    if changes.hotkey && tell_controller {
        // Reliable: if this were dropped, the saved key and the one the hook listens for would differ.
        host.send_reliably(Msg::Hotkey(settings.hotkey));
    }
    Ok((settings, changes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::widget::{Point, Style};
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A fresh directory path under the system temp dir; not created. Removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("shepit-settings-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join("settings.json")
        }

        fn bak(&self) -> PathBuf {
            self.0.join("settings.json.bak")
        }

        fn write(&self, text: &str) {
            fs::create_dir_all(&self.0).unwrap();
            fs::write(self.file(), text).unwrap();
        }

        fn names(&self) -> Vec<String> {
            let mut v: Vec<String> = fs::read_dir(&self.0)
                .map(|d| {
                    d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn changed() -> Settings {
        Settings {
            widget_style: Style::Island,
            glass_opacity: 0.8,
            fade_when_idle: false,
            widget_visible: false,
            widget_anchor: Some(Point { x: 100, y: 200 }),
            ..Settings::default()
        }
    }

    #[test]
    fn a_missing_file_loads_the_defaults_and_creates_nothing() {
        let t = TempDir::new();
        let mut store = SettingsStore::new(t.0.clone());
        assert_eq!(store.load(), Settings::default());
        assert!(!t.0.exists());
    }

    #[test]
    fn save_then_load_returns_the_same_settings() {
        let t = TempDir::new();
        let mut store = SettingsStore::new(t.0.clone());
        store.save(&changed()).unwrap();
        assert_eq!(SettingsStore::new(t.0.clone()).load(), changed());
    }

    #[test]
    fn save_creates_the_config_directory() {
        let t = TempDir::new();
        let mut store = SettingsStore::new(t.0.join("nested").join("deeper"));
        store.save(&Settings::default()).unwrap();
        assert!(t.0.join("nested/deeper/settings.json").is_file());
    }

    #[test]
    fn save_leaves_only_settings_json_behind() {
        let t = TempDir::new();
        let mut store = SettingsStore::new(t.0.clone());
        store.save(&changed()).unwrap();
        store.save(&Settings::default()).unwrap();
        assert_eq!(t.names(), ["settings.json"]);
    }

    #[test]
    fn save_replaces_an_existing_file() {
        let t = TempDir::new();
        t.write(r#"{"widgetVisible":false}"#);
        let mut store = SettingsStore::new(t.0.clone());
        let mut s = store.load();
        assert!(!s.widget_visible);
        s.widget_visible = true;
        store.save(&s).unwrap();
        assert!(SettingsStore::new(t.0.clone()).load().widget_visible);
    }

    #[test]
    fn a_valid_file_never_gets_a_backup() {
        let t = TempDir::new();
        t.write(r#"{"widgetVisible":false}"#);
        let mut store = SettingsStore::new(t.0.clone());
        let s = store.load();
        store.save(&s).unwrap();
        store.save(&s).unwrap();
        assert!(!t.bak().exists());
    }

    #[test]
    fn a_corrupt_file_loads_the_defaults() {
        let t = TempDir::new();
        t.write("{ this is not json");
        assert_eq!(SettingsStore::new(t.0.clone()).load(), Settings::default());
    }

    #[test]
    fn the_first_save_after_a_corrupt_load_backs_up_the_original_bytes() {
        let t = TempDir::new();
        t.write("{ this is not json");
        let mut store = SettingsStore::new(t.0.clone());
        store.load();
        store.save(&changed()).unwrap();
        assert_eq!(fs::read_to_string(t.bak()).unwrap(), "{ this is not json");
        assert_eq!(SettingsStore::new(t.0.clone()).load(), changed());
    }

    #[test]
    fn a_corrupt_file_that_vanished_before_the_save_does_not_block_saving() {
        let t = TempDir::new();
        t.write("{ this is not json");
        let mut store = SettingsStore::new(t.0.clone());
        store.load();
        fs::remove_file(t.0.join("settings.json")).unwrap();
        store.save(&changed()).unwrap();
        assert_eq!(t.names(), ["settings.json"]);
        store.save(&Settings::default()).unwrap();
        assert_eq!(SettingsStore::new(t.0.clone()).load(), Settings::default());
    }

    #[test]
    fn the_backup_is_made_once() {
        let t = TempDir::new();
        t.write("{ this is not json");
        let mut store = SettingsStore::new(t.0.clone());
        store.load();
        store.save(&changed()).unwrap();
        store.save(&Settings::default()).unwrap();
        store.save(&changed()).unwrap();
        assert_eq!(fs::read_to_string(t.bak()).unwrap(), "{ this is not json");
        assert_eq!(t.names(), ["settings.json", "settings.json.bak"]);
    }

    #[test]
    fn unknown_keys_survive_load_and_save() {
        let t = TempDir::new();
        t.write(r#"{"engine":"whisper-turbo","widgetVisible":false}"#);
        let mut store = SettingsStore::new(t.0.clone());
        let mut s = store.load();
        s.widget_visible = true;
        store.save(&s).unwrap();
        let text = fs::read_to_string(t.file()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["engine"], "whisper-turbo");
        assert_eq!(json["widgetVisible"], true);
    }
}
