//! The settings file of the Windows port: the widget keys of docs/spec/settings.md, read leniently
//! (each key falls back to its default on its own) and written with every other key kept.

use crate::keys::Hotkey;
use crate::polish::prompt::DEFAULT_SPEAKER_NOTES;
use crate::profile::Mode;
use crate::widget::{Point, Style};
use serde_json::{Map, Value};

pub const DEFAULT_GLASS_OPACITY: f64 = 0.7;
pub const MIN_GLASS_OPACITY: f64 = 0.5;
pub const MAX_GLASS_OPACITY: f64 = 0.9;

const KEY_STYLE: &str = "widgetStyle";
const KEY_OPACITY: &str = "glassOpacity";
const KEY_FADE: &str = "fadeWhenIdle";
const KEY_VISIBLE: &str = "widgetVisible";
const KEY_ANCHOR: &str = "widgetAnchor";
const KEY_HOTKEY: &str = "hotkey";
const KEY_ONBOARDING: &str = "onboardingCompleted";
const KEY_POLISHING: &str = "polishing";
const KEY_CLAUDE_MODEL: &str = "claudeModel";
const KEY_CLAUDE_EXE: &str = "claudeExecutable";
const KEY_CODEX_EXE: &str = "codexExecutable";
const KEY_DEFAULT_MODE: &str = "defaultMode";
const KEY_FAST_PATH: &str = "fastPathMaxWords";
const KEY_PROMPT_MIN: &str = "promptModeMinWords";
const KEY_NOTES: &str = "speakerNotes";
const KEY_DICTIONARY: &str = "dictionary";
const KEY_CHECK_UPDATES: &str = "checkForUpdates";
const KEY_SKIPPED_UPDATE: &str = "skippedUpdate";
const KEY_LAST_UPDATE_CHECK: &str = "lastUpdateCheck";
const KEY_LIVE_PREVIEW: &str = "livePreview";
const KNOWN_KEYS: [&str; 20] = [
    KEY_STYLE,
    KEY_OPACITY,
    KEY_FADE,
    KEY_VISIBLE,
    KEY_ANCHOR,
    KEY_HOTKEY,
    KEY_ONBOARDING,
    KEY_POLISHING,
    KEY_CLAUDE_MODEL,
    KEY_CLAUDE_EXE,
    KEY_CODEX_EXE,
    KEY_DEFAULT_MODE,
    KEY_FAST_PATH,
    KEY_PROMPT_MIN,
    KEY_NOTES,
    KEY_DICTIONARY,
    KEY_CHECK_UPDATES,
    KEY_SKIPPED_UPDATE,
    KEY_LAST_UPDATE_CHECK,
    KEY_LIVE_PREVIEW,
];

/// macOS does not clamp (AppSettings.swift:163); the Windows widget needs the glass readable
/// and never fully opaque, so a hand-edited value is pulled into the slider's range.
fn clamp_opacity(value: f64) -> f64 {
    value.clamp(MIN_GLASS_OPACITY, MAX_GLASS_OPACITY)
}

/// Which agent polishes the text (docs/spec/polishing.md, Provider setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polishing {
    Claude,
    Codex,
    Off,
}

impl Polishing {
    /// `claude`, `codex` or `off`.
    pub fn from_id(id: &str) -> Option<Polishing> {
        match id {
            "claude" => Some(Polishing::Claude),
            "codex" => Some(Polishing::Codex),
            "off" => Some(Polishing::Off),
            _ => None,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Polishing::Claude => "claude",
            Polishing::Codex => "codex",
            Polishing::Off => "off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeModel {
    Haiku,
    Sonnet,
}

impl ClaudeModel {
    /// `haiku` or `sonnet`.
    pub fn from_id(id: &str) -> Option<ClaudeModel> {
        match id {
            "haiku" => Some(ClaudeModel::Haiku),
            "sonnet" => Some(ClaudeModel::Sonnet),
            _ => None,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            ClaudeModel::Haiku => "haiku",
            ClaudeModel::Sonnet => "sonnet",
        }
    }
}

fn parse_anchor(value: &Value) -> Option<Point> {
    let coord = |key: &str| value.get(key)?.as_i64().and_then(|n| i32::try_from(n).ok());
    Some(Point {
        x: coord("x")?,
        y: coord("y")?,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub widget_style: Style,
    pub glass_opacity: f64,
    pub fade_when_idle: bool,
    pub widget_visible: bool,
    /// Persisted as `livePreview`. Show the words while speaking; read when a recording starts.
    pub live_preview: bool,
    /// Bottom centre of the widget, physical pixels. `None` means the default position.
    pub widget_anchor: Option<Point>,
    /// The dictation key. Persisted as `hotkey`: "rightCtrl", "rightAlt", "f13" to "f24".
    pub hotkey: Hotkey,
    /// Set once the welcome tour was finished, skipped or closed. Persisted as
    /// `onboardingCompleted`.
    pub onboarding_completed: bool,
    /// Persisted as `polishing`. Read leniently; the legacy `formatterEnabled` applies only when
    /// `polishing` is missing or not an id, and stays in `extra`.
    pub polishing: Polishing,
    pub claude_model: ClaudeModel,
    /// A configured path wins when it is an existing `.exe` or `.cmd`; empty means search.
    pub claude_executable: String,
    pub codex_executable: String,
    pub default_mode: Mode,
    /// Stored integers are not clamped on read; the patch clamps to 0-30.
    pub fast_path_max_words: i64,
    /// The patch clamps to 10-200.
    pub prompt_mode_min_words: i64,
    pub speaker_notes: String,
    /// Read-only here (task 10 edits it). A value holding a non-string reads as empty.
    pub dictionary: Vec<String>,
    /// Persisted as `checkForUpdates`: the daily look at GitHub.
    pub check_for_updates: bool,
    /// Persisted as `skippedUpdate`: the version the user chose to skip, or empty.
    pub skipped_update: String,
    /// Persisted as `lastUpdateCheck`: Unix seconds of the last successful check.
    pub last_update_check: Option<u64>,
    /// Keys this version does not know, written back unchanged.
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            widget_style: Style::FlowPill,
            glass_opacity: DEFAULT_GLASS_OPACITY,
            fade_when_idle: true,
            widget_visible: true,
            live_preview: true,
            widget_anchor: None,
            hotkey: Hotkey::RightCtrl,
            onboarding_completed: false,
            polishing: Polishing::Claude,
            claude_model: ClaudeModel::Haiku,
            claude_executable: String::new(),
            codex_executable: String::new(),
            default_mode: Mode::Clean,
            fast_path_max_words: 8,
            prompt_mode_min_words: 40,
            speaker_notes: DEFAULT_SPEAKER_NOTES.to_string(),
            dictionary: Vec::new(),
            check_for_updates: true,
            skipped_update: String::new(),
            last_update_check: None,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// The text was not a JSON object. `settings` is then the defaults.
    pub corrupt: bool,
}

impl Settings {
    pub fn parse(text: &str) -> Loaded {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
            return Loaded {
                settings: Settings::default(),
                corrupt: true,
            };
        };
        let d = Settings::default();
        let settings = Settings {
            widget_style: map
                .get(KEY_STYLE)
                .and_then(Value::as_str)
                .and_then(Style::from_id)
                .unwrap_or(d.widget_style),
            glass_opacity: map
                .get(KEY_OPACITY)
                .and_then(Value::as_f64)
                .map_or(d.glass_opacity, clamp_opacity),
            fade_when_idle: map
                .get(KEY_FADE)
                .and_then(Value::as_bool)
                .unwrap_or(d.fade_when_idle),
            widget_visible: map
                .get(KEY_VISIBLE)
                .and_then(Value::as_bool)
                .unwrap_or(d.widget_visible),
            live_preview: map
                .get(KEY_LIVE_PREVIEW)
                .and_then(Value::as_bool)
                .unwrap_or(d.live_preview),
            widget_anchor: map.get(KEY_ANCHOR).and_then(parse_anchor),
            hotkey: map
                .get(KEY_HOTKEY)
                .and_then(Value::as_str)
                .and_then(Hotkey::from_id)
                .unwrap_or(d.hotkey),
            onboarding_completed: map
                .get(KEY_ONBOARDING)
                .and_then(Value::as_bool)
                .unwrap_or(d.onboarding_completed),
            // A bad or missing `polishing` defers to the legacy on/off switch, which counts as on
            // unless it is the boolean false (AppSettings.swift:158-160).
            polishing: map
                .get(KEY_POLISHING)
                .and_then(Value::as_str)
                .and_then(Polishing::from_id)
                .unwrap_or_else(|| {
                    if map.get("formatterEnabled").and_then(Value::as_bool) == Some(false) {
                        Polishing::Off
                    } else {
                        Polishing::Claude
                    }
                }),
            claude_model: map
                .get(KEY_CLAUDE_MODEL)
                .and_then(Value::as_str)
                .and_then(ClaudeModel::from_id)
                .unwrap_or(d.claude_model),
            claude_executable: map
                .get(KEY_CLAUDE_EXE)
                .and_then(Value::as_str)
                .map_or(d.claude_executable, str::to_string),
            codex_executable: map
                .get(KEY_CODEX_EXE)
                .and_then(Value::as_str)
                .map_or(d.codex_executable, str::to_string),
            default_mode: map
                .get(KEY_DEFAULT_MODE)
                .and_then(Value::as_str)
                .and_then(Mode::from_id)
                .unwrap_or(d.default_mode),
            fast_path_max_words: map
                .get(KEY_FAST_PATH)
                .and_then(Value::as_i64)
                .unwrap_or(d.fast_path_max_words),
            prompt_mode_min_words: map
                .get(KEY_PROMPT_MIN)
                .and_then(Value::as_i64)
                .unwrap_or(d.prompt_mode_min_words),
            speaker_notes: map
                .get(KEY_NOTES)
                .and_then(Value::as_str)
                .map_or(d.speaker_notes, str::to_string),
            dictionary: map
                .get(KEY_DICTIONARY)
                .and_then(Value::as_array)
                .and_then(|a| {
                    a.iter()
                        .map(|v| v.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                })
                .unwrap_or(d.dictionary),
            check_for_updates: map
                .get(KEY_CHECK_UPDATES)
                .and_then(Value::as_bool)
                .unwrap_or(d.check_for_updates),
            skipped_update: map
                .get(KEY_SKIPPED_UPDATE)
                .and_then(Value::as_str)
                .map_or(d.skipped_update, str::to_string),
            // as_u64 rejects negatives, floats and strings, so they fall back to "never".
            last_update_check: map.get(KEY_LAST_UPDATE_CHECK).and_then(Value::as_u64),
            extra: map
                .into_iter()
                .filter(|(k, _)| !KNOWN_KEYS.contains(&k.as_str()))
                .collect(),
        };
        Loaded {
            settings,
            corrupt: false,
        }
    }

    /// Pretty JSON of the known keys plus `extra`. `glassOpacity` is clamped; an unset anchor is
    /// left out.
    pub fn to_json(&self) -> String {
        let mut map = self.extra.clone();
        map.insert(KEY_STYLE.into(), Value::from(self.widget_style.id()));
        map.insert(
            KEY_OPACITY.into(),
            Value::from(clamp_opacity(self.glass_opacity)),
        );
        map.insert(KEY_FADE.into(), Value::from(self.fade_when_idle));
        map.insert(KEY_VISIBLE.into(), Value::from(self.widget_visible));
        map.insert(KEY_LIVE_PREVIEW.into(), Value::from(self.live_preview));
        map.insert(KEY_HOTKEY.into(), Value::from(self.hotkey.id()));
        map.insert(
            KEY_ONBOARDING.into(),
            Value::from(self.onboarding_completed),
        );
        map.insert(KEY_POLISHING.into(), Value::from(self.polishing.id()));
        map.insert(KEY_CLAUDE_MODEL.into(), Value::from(self.claude_model.id()));
        map.insert(
            KEY_CLAUDE_EXE.into(),
            Value::from(self.claude_executable.as_str()),
        );
        map.insert(
            KEY_CODEX_EXE.into(),
            Value::from(self.codex_executable.as_str()),
        );
        map.insert(
            KEY_DEFAULT_MODE.into(),
            Value::from(self.default_mode.as_str()),
        );
        map.insert(KEY_FAST_PATH.into(), Value::from(self.fast_path_max_words));
        map.insert(
            KEY_PROMPT_MIN.into(),
            Value::from(self.prompt_mode_min_words),
        );
        map.insert(KEY_NOTES.into(), Value::from(self.speaker_notes.as_str()));
        map.insert(KEY_DICTIONARY.into(), Value::from(self.dictionary.clone()));
        map.insert(
            KEY_CHECK_UPDATES.into(),
            Value::from(self.check_for_updates),
        );
        map.insert(
            KEY_SKIPPED_UPDATE.into(),
            Value::from(self.skipped_update.as_str()),
        );
        match self.last_update_check {
            Some(t) => {
                map.insert(KEY_LAST_UPDATE_CHECK.into(), Value::from(t));
            }
            None => {
                map.remove(KEY_LAST_UPDATE_CHECK);
            }
        }
        match self.widget_anchor {
            Some(p) => {
                let mut anchor = Map::new();
                anchor.insert("x".into(), Value::from(p.x));
                anchor.insert("y".into(), Value::from(p.y));
                map.insert(KEY_ANCHOR.into(), Value::Object(anchor));
            }
            None => {
                map.remove(KEY_ANCHOR);
            }
        }
        // A map of plain values always serializes.
        serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default()
    }
}

/// What a settings write changed, so the app only touches what it must.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    /// `widgetStyle`, `glassOpacity` or `fadeWhenIdle`.
    pub look: bool,
    /// `hotkey`. The widget label and the hook follow it.
    pub hotkey: bool,
    /// `widgetVisible`.
    pub visible: bool,
    /// `polishing`, `claudeModel`, `speakerNotes`, `claudeExecutable` or `codexExecutable`. The
    /// agent is replaced 1.5 s after the last change.
    pub polishing: bool,
    /// `defaultMode`, `fastPathMaxWords`, `promptModeMinWords` or `dictionary`. Applies from the
    /// next dictation.
    pub formatting: bool,
    /// `checkForUpdates`. The skipped version and the last check time flag nothing.
    pub updates: bool,
    /// `dictionary`. Also flags `formatting`; this one tells the app the Dictionary pane has
    /// something new to show.
    pub dictionary: bool,
    /// `livePreview`. Read when the next recording starts.
    pub live_preview: bool,
}

impl Changes {
    pub fn any(&self) -> bool {
        self.look
            || self.hotkey
            || self.visible
            || self.polishing
            || self.formatting
            || self.updates
            || self.dictionary
            || self.live_preview
    }
}

/// Which of the UI-relevant settings differ between `old` and `new`.
pub fn changes(old: &Settings, new: &Settings) -> Changes {
    Changes {
        look: old.widget_style != new.widget_style
            || old.glass_opacity != new.glass_opacity
            || old.fade_when_idle != new.fade_when_idle,
        hotkey: old.hotkey != new.hotkey,
        visible: old.widget_visible != new.widget_visible,
        polishing: old.polishing != new.polishing
            || old.claude_model != new.claude_model
            || old.speaker_notes != new.speaker_notes
            || old.claude_executable != new.claude_executable
            || old.codex_executable != new.codex_executable,
        formatting: old.default_mode != new.default_mode
            || old.fast_path_max_words != new.fast_path_max_words
            || old.prompt_mode_min_words != new.prompt_mode_min_words
            || old.dictionary != new.dictionary,
        updates: old.check_for_updates != new.check_for_updates,
        dictionary: old.dictionary != new.dictionary,
        live_preview: old.live_preview != new.live_preview,
    }
}

fn patch_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{key}: not a string"))
}

/// Applies a UI edit. The patch is a JSON object with any of `widgetStyle`, `glassOpacity`,
/// `fadeWhenIdle`, `widgetVisible`, `livePreview`, `hotkey`, `checkForUpdates` and the polishing keys and
/// `dictionary`, which is split on line ends, trimmed and cleared of blanks (not
/// `skippedUpdate` or `lastUpdateCheck`, which only the updater writes). An unknown
/// key, a wrong type or a bad id rejects the whole patch and leaves `current` as it was. `glassOpacity` is clamped like a loaded file.
pub fn apply_patch(current: &Settings, patch: &Value) -> Result<Settings, String> {
    let Value::Object(map) = patch else {
        return Err("the patch must be an object".into());
    };
    // Built on a copy so a rejected patch leaves `current` untouched.
    let mut out = current.clone();
    for (key, value) in map {
        match key.as_str() {
            KEY_STYLE => {
                out.widget_style = value
                    .as_str()
                    .and_then(Style::from_id)
                    .ok_or_else(|| format!("{KEY_STYLE}: unknown style"))?;
            }
            KEY_OPACITY => {
                out.glass_opacity = value
                    .as_f64()
                    .map(clamp_opacity)
                    .ok_or_else(|| format!("{KEY_OPACITY}: not a number"))?;
            }
            KEY_FADE => {
                out.fade_when_idle = value
                    .as_bool()
                    .ok_or_else(|| format!("{KEY_FADE}: not a boolean"))?;
            }
            KEY_VISIBLE => {
                out.widget_visible = value
                    .as_bool()
                    .ok_or_else(|| format!("{KEY_VISIBLE}: not a boolean"))?;
            }
            KEY_LIVE_PREVIEW => {
                out.live_preview = value
                    .as_bool()
                    .ok_or_else(|| format!("{KEY_LIVE_PREVIEW}: not a boolean"))?;
            }
            KEY_HOTKEY => {
                out.hotkey = value
                    .as_str()
                    .and_then(Hotkey::from_id)
                    .ok_or_else(|| format!("{KEY_HOTKEY}: unknown key"))?;
            }
            KEY_POLISHING => {
                out.polishing = value
                    .as_str()
                    .and_then(Polishing::from_id)
                    .ok_or_else(|| format!("{KEY_POLISHING}: unknown provider"))?;
            }
            KEY_CLAUDE_MODEL => {
                out.claude_model = value
                    .as_str()
                    .and_then(ClaudeModel::from_id)
                    .ok_or_else(|| format!("{KEY_CLAUDE_MODEL}: unknown model"))?;
            }
            KEY_CLAUDE_EXE => {
                out.claude_executable = patch_string(value, KEY_CLAUDE_EXE)?;
            }
            KEY_CODEX_EXE => {
                out.codex_executable = patch_string(value, KEY_CODEX_EXE)?;
            }
            KEY_DEFAULT_MODE => {
                out.default_mode = value
                    .as_str()
                    .and_then(Mode::from_id)
                    .ok_or_else(|| format!("{KEY_DEFAULT_MODE}: unknown mode"))?;
            }
            // The steppers of the macOS settings window (SettingsView.swift:39,160).
            KEY_FAST_PATH => {
                out.fast_path_max_words = value
                    .as_i64()
                    .map(|n| n.clamp(0, 30))
                    .ok_or_else(|| format!("{KEY_FAST_PATH}: not an integer"))?;
            }
            KEY_PROMPT_MIN => {
                out.prompt_mode_min_words = value
                    .as_i64()
                    .map(|n| n.clamp(10, 200))
                    .ok_or_else(|| format!("{KEY_PROMPT_MIN}: not an integer"))?;
            }
            KEY_NOTES => {
                out.speaker_notes = patch_string(value, KEY_NOTES)?;
            }
            KEY_DICTIONARY => {
                let items = value
                    .as_array()
                    .and_then(|a| {
                        a.iter()
                            .map(|v| v.as_str().map(str::to_string))
                            .collect::<Option<Vec<String>>>()
                    })
                    .ok_or_else(|| format!("{KEY_DICTIONARY}: not an array of strings"))?;
                out.dictionary = crate::dictionary::normalize_terms(&items);
            }
            KEY_CHECK_UPDATES => {
                out.check_for_updates = value
                    .as_bool()
                    .ok_or_else(|| format!("{KEY_CHECK_UPDATES}: not a boolean"))?;
            }
            KEY_SKIPPED_UPDATE | KEY_LAST_UPDATE_CHECK => {
                return Err(format!("{key}: cannot be patched here"));
            }
            // widgetAnchor and onboardingCompleted have their own commands.
            other => return Err(format!("unknown setting: {other}")),
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Settings {
        let loaded = Settings::parse(text);
        assert!(!loaded.corrupt, "{text} was marked corrupt");
        loaded.settings
    }

    fn written(s: &Settings) -> Value {
        serde_json::from_str(&s.to_json()).expect("to_json writes valid JSON")
    }

    #[test]
    fn an_empty_object_gives_the_defaults() {
        assert_eq!(parse("{}"), Settings::default());
    }

    #[test]
    fn the_defaults_match_the_spec() {
        let d = Settings::default();
        assert_eq!(d.widget_style, Style::FlowPill);
        assert_eq!(d.glass_opacity, 0.7);
        assert!(d.fade_when_idle);
        assert!(d.widget_visible);
        assert_eq!(d.widget_anchor, None);
        assert!(d.extra.is_empty());
    }

    #[test]
    fn valid_values_are_read() {
        let s = parse(
            r#"{"widgetStyle":"island","glassOpacity":0.55,"fadeWhenIdle":false,
                "widgetVisible":false,"widgetAnchor":{"x":-640,"y":976}}"#,
        );
        assert_eq!(s.widget_style, Style::Island);
        assert_eq!(s.glass_opacity, 0.55);
        assert!(!s.fade_when_idle);
        assert!(!s.widget_visible);
        assert_eq!(s.widget_anchor, Some(Point { x: -640, y: 976 }));
    }

    #[test]
    fn a_wrong_widget_style_falls_back_alone() {
        for bad in ["5", "\"bogus\"", "null", "true", "[]"] {
            let s = parse(&format!(r#"{{"widgetStyle":{bad},"fadeWhenIdle":false}}"#));
            assert_eq!(s.widget_style, Style::FlowPill, "{bad}");
            assert!(!s.fade_when_idle, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn a_wrong_glass_opacity_falls_back_alone() {
        for bad in ["\"0.7\"", "null", "true", "{}"] {
            let s = parse(&format!(
                r#"{{"glassOpacity":{bad},"widgetVisible":false}}"#
            ));
            assert_eq!(s.glass_opacity, 0.7, "{bad}");
            assert!(!s.widget_visible, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn a_wrong_fade_when_idle_falls_back_alone() {
        for bad in ["\"no\"", "0", "null", "[]"] {
            let s = parse(&format!(
                r#"{{"fadeWhenIdle":{bad},"widgetVisible":false}}"#
            ));
            assert!(s.fade_when_idle, "{bad}");
            assert!(!s.widget_visible, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn a_wrong_widget_visible_falls_back_alone() {
        for bad in ["\"no\"", "0", "null", "{}"] {
            let s = parse(&format!(
                r#"{{"widgetVisible":{bad},"fadeWhenIdle":false}}"#
            ));
            assert!(s.widget_visible, "{bad}");
            assert!(!s.fade_when_idle, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn glass_opacity_is_clamped_to_0_5_through_0_9() {
        assert_eq!(parse(r#"{"glassOpacity":0.2}"#).glass_opacity, 0.5);
        assert_eq!(parse(r#"{"glassOpacity":1.5}"#).glass_opacity, 0.9);
        assert_eq!(parse(r#"{"glassOpacity":-3}"#).glass_opacity, 0.5);
        assert_eq!(parse(r#"{"glassOpacity":0.5}"#).glass_opacity, 0.5);
        assert_eq!(parse(r#"{"glassOpacity":0.9}"#).glass_opacity, 0.9);
        assert_eq!(parse(r#"{"glassOpacity":1}"#).glass_opacity, 0.9);
    }

    #[test]
    fn an_out_of_range_opacity_is_written_clamped() {
        let high = Settings {
            glass_opacity: 2.0,
            ..Settings::default()
        };
        assert_eq!(written(&high)["glassOpacity"], 0.9);
        let low = Settings {
            glass_opacity: 0.0,
            ..Settings::default()
        };
        assert_eq!(written(&low)["glassOpacity"], 0.5);
    }

    #[test]
    fn a_missing_anchor_is_none() {
        assert_eq!(parse(r#"{"widgetVisible":true}"#).widget_anchor, None);
    }

    #[test]
    fn a_malformed_anchor_is_none_and_spares_the_other_keys() {
        for bad in [
            "null",
            "\"1,2\"",
            "[1,2]",
            "{}",
            r#"{"x":1}"#,
            r#"{"y":1}"#,
            r#"{"x":"1","y":2}"#,
            r#"{"x":1,"y":null}"#,
        ] {
            let s = parse(&format!(r#"{{"widgetAnchor":{bad},"fadeWhenIdle":false}}"#));
            assert_eq!(s.widget_anchor, None, "{bad}");
            assert!(!s.fade_when_idle, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let text = r#"{"engine":"whisper-turbo","future":{"a":[1,2]},"widgetVisible":false}"#;
        let mut s = parse(text);
        s.fade_when_idle = false;
        let out = written(&s);
        assert_eq!(out["engine"], "whisper-turbo");
        assert_eq!(out["future"]["a"][1], 2);
        assert_eq!(out["widgetVisible"], false);
        assert_eq!(out["fadeWhenIdle"], false);
    }

    #[test]
    fn known_keys_use_the_spec_names() {
        let s = Settings {
            widget_anchor: Some(Point { x: 960, y: 1032 }),
            ..Settings::default()
        };
        let out = written(&s);
        assert_eq!(out["widgetStyle"], "flowPill");
        assert_eq!(out["glassOpacity"], 0.7);
        assert_eq!(out["fadeWhenIdle"], true);
        assert_eq!(out["widgetVisible"], true);
        assert_eq!(out["widgetAnchor"]["x"], 960);
        assert_eq!(out["widgetAnchor"]["y"], 1032);
    }

    #[test]
    fn an_unset_anchor_is_not_written() {
        assert!(written(&Settings::default()).get("widgetAnchor").is_none());
    }

    #[test]
    fn write_then_parse_gives_the_same_settings() {
        let mut s = Settings {
            widget_style: Style::CaptionBar,
            glass_opacity: 0.85,
            fade_when_idle: false,
            widget_visible: false,
            widget_anchor: Some(Point { x: -5, y: 7 }),
            ..Settings::default()
        };
        s.extra.insert("engine".into(), Value::from("parakeet-v3"));
        assert_eq!(parse(&s.to_json()), s);
    }

    #[test]
    fn invalid_json_is_corrupt_and_gives_the_defaults() {
        for bad in [
            "{",
            "not json",
            "{\"widgetVisible\":",
            "[1,2]",
            "42",
            "\"x\"",
            "null",
        ] {
            let loaded = Settings::parse(bad);
            assert!(loaded.corrupt, "{bad}");
            assert_eq!(loaded.settings, Settings::default(), "{bad}");
        }
    }

    #[test]
    fn valid_json_is_not_corrupt() {
        assert!(!Settings::parse("{}").corrupt);
        assert!(!Settings::parse(r#"{"widgetStyle":5}"#).corrupt);
    }

    // Hotkey and onboarding.

    #[test]
    fn the_hotkey_defaults_to_right_ctrl_and_the_tour_is_not_done() {
        let d = Settings::default();
        assert_eq!(d.hotkey, Hotkey::RightCtrl);
        assert!(!d.onboarding_completed);
        assert_eq!(parse("{}").hotkey, Hotkey::RightCtrl);
        assert!(!parse("{}").onboarding_completed);
    }

    #[test]
    fn a_stored_hotkey_is_read_for_every_id() {
        for h in Hotkey::ALL {
            let s = parse(&format!(r#"{{"hotkey":"{}"}}"#, h.id()));
            assert_eq!(s.hotkey, h);
        }
    }

    #[test]
    fn a_bad_hotkey_falls_back_alone() {
        for bad in [
            "5",
            "\"bogus\"",
            "\"RightCtrl\"",
            "\"leftCtrl\"",
            "null",
            "true",
            "[]",
        ] {
            let s = parse(&format!(r#"{{"hotkey":{bad},"fadeWhenIdle":false}}"#));
            assert_eq!(s.hotkey, Hotkey::RightCtrl, "{bad}");
            assert!(!s.fade_when_idle, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn onboarding_completed_is_read_and_a_wrong_type_falls_back_alone() {
        assert!(parse(r#"{"onboardingCompleted":true}"#).onboarding_completed);
        for bad in ["\"yes\"", "1", "null", "[]"] {
            let s = parse(&format!(
                r#"{{"onboardingCompleted":{bad},"widgetVisible":false}}"#
            ));
            assert!(!s.onboarding_completed, "{bad}");
            assert!(!s.widget_visible, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn the_new_keys_are_written_with_the_spec_names() {
        let s = Settings {
            hotkey: Hotkey::F17,
            onboarding_completed: true,
            ..Settings::default()
        };
        let out = written(&s);
        assert_eq!(out["hotkey"], "f17");
        assert_eq!(out["onboardingCompleted"], true);
        let d = written(&Settings::default());
        assert_eq!(d["hotkey"], "rightCtrl");
        assert_eq!(d["onboardingCompleted"], false);
    }

    #[test]
    fn the_new_keys_are_not_kept_as_extras() {
        let s = parse(r#"{"hotkey":"rightAlt","onboardingCompleted":true,"engine":"x"}"#);
        assert!(!s.extra.contains_key("hotkey"));
        assert!(!s.extra.contains_key("onboardingCompleted"));
        assert!(s.extra.contains_key("engine"));
    }

    #[test]
    fn write_then_parse_keeps_the_hotkey_and_the_tour_flag() {
        let s = Settings {
            hotkey: Hotkey::RightAlt,
            onboarding_completed: true,
            ..Settings::default()
        };
        assert_eq!(parse(&s.to_json()), s);
    }

    // apply_patch.

    fn patch(current: &Settings, text: &str) -> Result<Settings, String> {
        apply_patch(current, &serde_json::from_str::<Value>(text).unwrap())
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let s = Settings {
            widget_style: Style::Island,
            widget_anchor: Some(Point { x: 1, y: 2 }),
            ..Settings::default()
        };
        assert_eq!(patch(&s, "{}"), Ok(s));
    }

    #[test]
    fn a_patch_sets_the_widget_style() {
        let out = patch(&Settings::default(), r#"{"widgetStyle":"captionBar"}"#).unwrap();
        assert_eq!(out.widget_style, Style::CaptionBar);
    }

    #[test]
    fn a_patch_sets_the_glass_opacity() {
        let out = patch(&Settings::default(), r#"{"glassOpacity":0.55}"#).unwrap();
        assert_eq!(out.glass_opacity, 0.55);
    }

    #[test]
    fn a_patch_sets_fade_when_idle() {
        let out = patch(&Settings::default(), r#"{"fadeWhenIdle":false}"#).unwrap();
        assert!(!out.fade_when_idle);
    }

    #[test]
    fn a_patch_sets_widget_visible() {
        let out = patch(&Settings::default(), r#"{"widgetVisible":false}"#).unwrap();
        assert!(!out.widget_visible);
    }

    #[test]
    fn a_patch_sets_the_hotkey_for_every_id() {
        for h in Hotkey::ALL {
            let out = patch(
                &Settings::default(),
                &format!(r#"{{"hotkey":"{}"}}"#, h.id()),
            );
            assert_eq!(out.unwrap().hotkey, h);
        }
    }

    #[test]
    fn a_patch_touches_only_the_keys_it_names() {
        let mut current = Settings {
            widget_style: Style::Island,
            glass_opacity: 0.6,
            fade_when_idle: false,
            widget_visible: false,
            widget_anchor: Some(Point { x: 9, y: 8 }),
            hotkey: Hotkey::F14,
            onboarding_completed: true,
            ..Settings::default()
        };
        current
            .extra
            .insert("engine".into(), Value::from("parakeet-v3"));
        let out = patch(&current, r#"{"fadeWhenIdle":true}"#).unwrap();
        assert_eq!(
            out,
            Settings {
                fade_when_idle: true,
                ..current
            }
        );
    }

    #[test]
    fn a_patch_clamps_the_glass_opacity() {
        let high = patch(&Settings::default(), r#"{"glassOpacity":3}"#).unwrap();
        assert_eq!(high.glass_opacity, 0.9);
        let low = patch(&Settings::default(), r#"{"glassOpacity":-1.5}"#).unwrap();
        assert_eq!(low.glass_opacity, 0.5);
    }

    #[test]
    fn an_unknown_key_rejects_the_whole_patch() {
        let r = patch(&Settings::default(), r#"{"widgetVisible":false,"bogus":1}"#);
        assert!(r.is_err());
    }

    #[test]
    fn the_anchor_and_the_tour_flag_cannot_be_patched() {
        let anchor = patch(&Settings::default(), r#"{"widgetAnchor":{"x":1,"y":2}}"#);
        assert!(anchor.is_err());
        let tour = patch(&Settings::default(), r#"{"onboardingCompleted":true}"#);
        assert!(tour.is_err());
    }

    #[test]
    fn a_wrong_type_rejects_the_whole_patch() {
        for bad in [
            r#"{"widgetStyle":5}"#,
            r#"{"widgetStyle":null}"#,
            r#"{"glassOpacity":"0.7"}"#,
            r#"{"glassOpacity":null}"#,
            r#"{"fadeWhenIdle":"no"}"#,
            r#"{"fadeWhenIdle":0}"#,
            r#"{"widgetVisible":null}"#,
            r#"{"widgetVisible":[]}"#,
            r#"{"hotkey":5}"#,
            r#"{"hotkey":null}"#,
            r#"{"hotkey":["f13"]}"#,
        ] {
            assert!(patch(&Settings::default(), bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_bad_id_rejects_the_whole_patch() {
        for bad in [
            r#"{"widgetStyle":"bogus"}"#,
            r#"{"hotkey":"leftCtrl"}"#,
            r#"{"hotkey":"f12"}"#,
            r#"{"hotkey":"RightCtrl"}"#,
            r#"{"hotkey":""}"#,
        ] {
            assert!(patch(&Settings::default(), bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn one_bad_value_rejects_the_good_ones_beside_it() {
        let r = patch(
            &Settings::default(),
            r#"{"widgetVisible":false,"hotkey":"nope","fadeWhenIdle":false}"#,
        );
        assert!(r.is_err());
    }

    #[test]
    fn a_patch_that_is_not_an_object_is_rejected() {
        for bad in ["null", "[]", "5", "\"x\"", "true"] {
            assert!(patch(&Settings::default(), bad).is_err(), "{bad}");
        }
    }

    // changes.

    #[test]
    fn identical_settings_change_nothing() {
        let s = Settings::default();
        assert_eq!(changes(&s, &s.clone()), Changes::default());
        assert!(!changes(&s, &s.clone()).any());
    }

    #[test]
    fn each_look_setting_flags_look_only() {
        let old = Settings::default();
        let edits = [
            Settings {
                widget_style: Style::Island,
                ..old.clone()
            },
            Settings {
                glass_opacity: 0.55,
                ..old.clone()
            },
            Settings {
                fade_when_idle: false,
                ..old.clone()
            },
        ];
        for new in edits {
            assert_eq!(
                changes(&old, &new),
                Changes {
                    look: true,
                    ..Changes::default()
                }
            );
        }
    }

    #[test]
    fn a_new_hotkey_flags_hotkey_only() {
        let old = Settings::default();
        let new = Settings {
            hotkey: Hotkey::RightAlt,
            ..old.clone()
        };
        assert_eq!(
            changes(&old, &new),
            Changes {
                hotkey: true,
                ..Changes::default()
            }
        );
    }

    #[test]
    fn a_visibility_change_flags_visible_only() {
        let old = Settings::default();
        let new = Settings {
            widget_visible: false,
            ..old.clone()
        };
        let c = changes(&old, &new);
        assert_eq!(
            c,
            Changes {
                visible: true,
                ..Changes::default()
            }
        );
        assert!(c.any());
    }

    #[test]
    fn the_anchor_the_tour_flag_and_extras_flag_nothing() {
        let old = Settings::default();
        let mut new = Settings {
            widget_anchor: Some(Point { x: 3, y: 4 }),
            onboarding_completed: true,
            ..old.clone()
        };
        new.extra.insert("engine".into(), Value::from("x"));
        assert_eq!(changes(&old, &new), Changes::default());
    }

    #[test]
    fn several_edits_flag_several_fields() {
        let old = Settings::default();
        let new = Settings {
            widget_style: Style::CaptionBar,
            hotkey: Hotkey::F20,
            widget_visible: false,
            ..old.clone()
        };
        assert_eq!(
            changes(&old, &new),
            Changes {
                look: true,
                hotkey: true,
                visible: true,
                ..Changes::default()
            }
        );
    }

    // Polishing keys (docs/spec/settings.md, polishing.md).

    fn patch_with(current: &Settings, text: &str) -> Result<Settings, String> {
        apply_patch(current, &serde_json::from_str::<Value>(text).unwrap())
    }

    #[test]
    fn the_polishing_defaults_match_the_spec() {
        let s = parse("{}");
        assert_eq!(s.polishing, Polishing::Claude);
        assert_eq!(s.claude_model, ClaudeModel::Haiku);
        assert_eq!(s.claude_executable, "");
        assert_eq!(s.codex_executable, "");
        assert_eq!(s.default_mode, Mode::Clean);
        assert_eq!(s.fast_path_max_words, 8);
        assert_eq!(s.prompt_mode_min_words, 40);
        assert_eq!(s.speaker_notes, DEFAULT_SPEAKER_NOTES);
        assert!(s.dictionary.is_empty());
    }

    #[test]
    fn valid_polishing_values_are_read() {
        let s = parse(
            r#"{"polishing":"codex","claudeModel":"sonnet","claudeExecutable":"C:\\bin\\claude.exe",
                "codexExecutable":"C:\\bin\\codex.cmd","defaultMode":"prompt","fastPathMaxWords":12,
                "promptModeMinWords":55,"speakerNotes":"My notes","dictionary":["Shepit","Vercel"]}"#,
        );
        assert_eq!(s.polishing, Polishing::Codex);
        assert_eq!(s.claude_model, ClaudeModel::Sonnet);
        assert_eq!(s.claude_executable, r"C:\bin\claude.exe");
        assert_eq!(s.codex_executable, r"C:\bin\codex.cmd");
        assert_eq!(s.default_mode, Mode::Prompt);
        assert_eq!(s.fast_path_max_words, 12);
        assert_eq!(s.prompt_mode_min_words, 55);
        assert_eq!(s.speaker_notes, "My notes");
        assert_eq!(s.dictionary, ["Shepit", "Vercel"]);
    }

    #[test]
    fn every_polishing_id_is_read() {
        for (id, want) in [
            ("claude", Polishing::Claude),
            ("codex", Polishing::Codex),
            ("off", Polishing::Off),
        ] {
            assert_eq!(parse(&format!(r#"{{"polishing":"{id}"}}"#)).polishing, want);
            assert_eq!(want.id(), id);
        }
        for (id, want) in [
            ("haiku", ClaudeModel::Haiku),
            ("sonnet", ClaudeModel::Sonnet),
        ] {
            assert_eq!(
                parse(&format!(r#"{{"claudeModel":"{id}"}}"#)).claude_model,
                want
            );
            assert_eq!(want.id(), id);
        }
    }

    #[test]
    fn a_wrong_polishing_value_falls_back_alone() {
        for bad in ["5", "\"bogus\"", "null", "true", "[]"] {
            let s = parse(&format!(r#"{{"polishing":{bad},"claudeModel":"sonnet"}}"#));
            assert_eq!(s.polishing, Polishing::Claude, "{bad}");
            assert_eq!(
                s.claude_model,
                ClaudeModel::Sonnet,
                "{bad} must not touch other keys"
            );
        }
    }

    #[test]
    fn a_wrong_claude_model_falls_back_alone() {
        for bad in ["5", "\"opus\"", "null", "{}"] {
            let s = parse(&format!(r#"{{"claudeModel":{bad},"defaultMode":"raw"}}"#));
            assert_eq!(s.claude_model, ClaudeModel::Haiku, "{bad}");
            assert_eq!(s.default_mode, Mode::Raw, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn a_wrong_executable_falls_back_to_empty_alone() {
        for bad in ["5", "null", "true", "[]"] {
            let s = parse(&format!(
                r#"{{"claudeExecutable":{bad},"codexExecutable":"C:\\c.exe"}}"#
            ));
            assert_eq!(s.claude_executable, "", "{bad}");
            assert_eq!(
                s.codex_executable, r"C:\c.exe",
                "{bad} must not touch other keys"
            );
        }
    }

    #[test]
    fn a_wrong_default_mode_falls_back_alone() {
        for bad in ["5", "\"loud\"", "null", "[]"] {
            let s = parse(&format!(r#"{{"defaultMode":{bad},"fastPathMaxWords":3}}"#));
            assert_eq!(s.default_mode, Mode::Clean, "{bad}");
            assert_eq!(s.fast_path_max_words, 3, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn a_wrong_word_limit_falls_back_alone() {
        for bad in ["\"8\"", "null", "true", "2.5", "[]"] {
            let s = parse(&format!(
                r#"{{"fastPathMaxWords":{bad},"promptModeMinWords":{bad},"speakerNotes":"n"}}"#
            ));
            assert_eq!(s.fast_path_max_words, 8, "{bad}");
            assert_eq!(s.prompt_mode_min_words, 40, "{bad}");
            assert_eq!(s.speaker_notes, "n", "{bad} must not touch other keys");
        }
    }

    #[test]
    fn stored_integers_are_not_clamped() {
        let s = parse(r#"{"fastPathMaxWords":-5,"promptModeMinWords":9999}"#);
        assert_eq!(s.fast_path_max_words, -5);
        assert_eq!(s.prompt_mode_min_words, 9999);
    }

    #[test]
    fn wrong_speaker_notes_fall_back_to_the_default_text() {
        for bad in ["5", "null", "[]"] {
            let s = parse(&format!(r#"{{"speakerNotes":{bad}}}"#));
            assert_eq!(s.speaker_notes, DEFAULT_SPEAKER_NOTES, "{bad}");
        }
    }

    #[test]
    fn empty_speaker_notes_stay_empty() {
        assert_eq!(parse(r#"{"speakerNotes":""}"#).speaker_notes, "");
    }

    #[test]
    fn a_dictionary_holding_a_non_string_is_empty() {
        for bad in [r#"["A",2]"#, r#"[null]"#, r#""A""#, "5", r#"{"a":1}"#] {
            let s = parse(&format!(r#"{{"dictionary":{bad},"defaultMode":"raw"}}"#));
            assert!(s.dictionary.is_empty(), "{bad}");
            assert_eq!(s.default_mode, Mode::Raw, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn the_legacy_switch_applies_only_when_polishing_is_missing() {
        assert_eq!(
            parse(r#"{"formatterEnabled":true}"#).polishing,
            Polishing::Claude
        );
        assert_eq!(
            parse(r#"{"formatterEnabled":false}"#).polishing,
            Polishing::Off
        );
        assert_eq!(parse("{}").polishing, Polishing::Claude);
        assert_eq!(
            parse(r#"{"formatterEnabled":false,"polishing":"codex"}"#).polishing,
            Polishing::Codex
        );
        assert_eq!(
            parse(r#"{"formatterEnabled":true,"polishing":"off"}"#).polishing,
            Polishing::Off
        );
    }

    #[test]
    fn a_bad_polishing_value_defers_to_the_legacy_switch() {
        assert_eq!(
            parse(r#"{"formatterEnabled":false,"polishing":"bogus"}"#).polishing,
            Polishing::Off
        );
        assert_eq!(
            parse(r#"{"formatterEnabled":true,"polishing":7}"#).polishing,
            Polishing::Claude
        );
    }

    #[test]
    fn a_wrong_legacy_value_counts_as_on() {
        assert_eq!(
            parse(r#"{"formatterEnabled":"no"}"#).polishing,
            Polishing::Claude
        );
    }

    #[test]
    fn the_legacy_key_stays_in_extra_and_survives_a_write() {
        let s = parse(r#"{"formatterEnabled":false}"#);
        assert_eq!(s.extra.get("formatterEnabled"), Some(&Value::Bool(false)));
        assert_eq!(written(&s)["formatterEnabled"], Value::Bool(false));
    }

    #[test]
    fn the_polishing_keys_are_written_with_the_spec_names() {
        let mut s = Settings::default();
        s.polishing = Polishing::Off;
        s.claude_model = ClaudeModel::Sonnet;
        s.claude_executable = r"C:\a\claude.exe".into();
        s.codex_executable = r"C:\a\codex.cmd".into();
        s.default_mode = Mode::Raw;
        s.fast_path_max_words = 3;
        s.prompt_mode_min_words = 60;
        s.speaker_notes = "notes".into();
        s.dictionary = vec!["Shepit".into()];
        let w = written(&s);
        assert_eq!(w["polishing"], "off");
        assert_eq!(w["claudeModel"], "sonnet");
        assert_eq!(w["claudeExecutable"], r"C:\a\claude.exe");
        assert_eq!(w["codexExecutable"], r"C:\a\codex.cmd");
        assert_eq!(w["defaultMode"], "raw");
        assert_eq!(w["fastPathMaxWords"], 3);
        assert_eq!(w["promptModeMinWords"], 60);
        assert_eq!(w["speakerNotes"], "notes");
        assert_eq!(w["dictionary"], serde_json::json!(["Shepit"]));
    }

    #[test]
    fn the_polishing_keys_are_not_kept_as_extras() {
        let s = parse(
            r#"{"polishing":"off","claudeModel":"sonnet","claudeExecutable":"x.exe","codexExecutable":"y.exe",
                "defaultMode":"raw","fastPathMaxWords":1,"promptModeMinWords":2,"speakerNotes":"n","dictionary":["A"]}"#,
        );
        assert!(s.extra.is_empty(), "{:?}", s.extra);
    }

    #[test]
    fn write_then_parse_keeps_the_polishing_settings() {
        let mut s = Settings::default();
        s.polishing = Polishing::Codex;
        s.claude_model = ClaudeModel::Sonnet;
        s.default_mode = Mode::Prompt;
        s.fast_path_max_words = 0;
        s.prompt_mode_min_words = 150;
        s.speaker_notes = "Привіт, notes".into();
        s.dictionary = vec!["Shepit".into(), "Vercel".into()];
        assert_eq!(parse(&s.to_json()), s);
    }

    #[test]
    fn a_patch_sets_every_polishing_key() {
        let base = Settings::default();
        let s = patch_with(
            &base,
            r#"{"polishing":"codex","claudeModel":"sonnet","claudeExecutable":"C:\\c\\claude.exe",
                "codexExecutable":"C:\\c\\codex.cmd","defaultMode":"raw","fastPathMaxWords":5,
                "promptModeMinWords":80,"speakerNotes":"mine"}"#,
        )
        .unwrap();
        assert_eq!(s.polishing, Polishing::Codex);
        assert_eq!(s.claude_model, ClaudeModel::Sonnet);
        assert_eq!(s.claude_executable, r"C:\c\claude.exe");
        assert_eq!(s.codex_executable, r"C:\c\codex.cmd");
        assert_eq!(s.default_mode, Mode::Raw);
        assert_eq!(s.fast_path_max_words, 5);
        assert_eq!(s.prompt_mode_min_words, 80);
        assert_eq!(s.speaker_notes, "mine");
    }

    #[test]
    fn a_patch_clamps_the_word_limits() {
        let base = Settings::default();
        let s = patch_with(&base, r#"{"fastPathMaxWords":-4,"promptModeMinWords":3}"#).unwrap();
        assert_eq!((s.fast_path_max_words, s.prompt_mode_min_words), (0, 10));
        let s = patch_with(&base, r#"{"fastPathMaxWords":99,"promptModeMinWords":900}"#).unwrap();
        assert_eq!((s.fast_path_max_words, s.prompt_mode_min_words), (30, 200));
        let s = patch_with(&base, r#"{"fastPathMaxWords":30,"promptModeMinWords":10}"#).unwrap();
        assert_eq!((s.fast_path_max_words, s.prompt_mode_min_words), (30, 10));
    }

    #[test]
    fn a_patch_rejects_wrong_types_and_bad_ids() {
        let base = Settings::default();
        for bad in [
            r#"{"polishing":"bogus"}"#,
            r#"{"polishing":1}"#,
            r#"{"claudeModel":"opus"}"#,
            r#"{"defaultMode":"loud"}"#,
            r#"{"fastPathMaxWords":"8"}"#,
            r#"{"fastPathMaxWords":2.5}"#,
            r#"{"promptModeMinWords":null}"#,
            r#"{"claudeExecutable":5}"#,
            r#"{"codexExecutable":null}"#,
            r#"{"speakerNotes":false}"#,
        ] {
            assert!(patch_with(&base, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_patch_sets_the_dictionary_split_trimmed_and_without_blanks() {
        let base = Settings::default();
        let s = patch_with(
            &base,
            r#"{"dictionary":["  Vercel ","A\r\nB","","  ","C\rD","E\nF"]}"#,
        )
        .unwrap();
        assert_eq!(s.dictionary, ["Vercel", "A", "B", "C", "D", "E", "F"]);
    }

    #[test]
    fn a_dictionary_patch_keeps_order_and_duplicates_and_replaces_the_list() {
        let mut base = Settings::default();
        base.dictionary = vec!["Old".into()];
        let s = patch_with(&base, r#"{"dictionary":["B","a","B"]}"#).unwrap();
        assert_eq!(s.dictionary, ["B", "a", "B"]);
        let s = patch_with(&base, r#"{"dictionary":[]}"#).unwrap();
        assert!(s.dictionary.is_empty());
    }

    #[test]
    fn a_dictionary_patch_of_the_wrong_type_rejects_the_whole_patch() {
        let mut base = Settings::default();
        base.dictionary = vec!["Keep".into()];
        for bad in [
            r#"{"dictionary":"A"}"#,
            r#"{"dictionary":null}"#,
            r#"{"dictionary":5}"#,
            r#"{"dictionary":{"a":1}}"#,
            r#"{"dictionary":["A",2]}"#,
            r#"{"dictionary":["A",null]}"#,
            r#"{"dictionary":["A",["B"]]}"#,
            r#"{"speakerNotes":"x","dictionary":"A"}"#,
        ] {
            let err = patch_with(&base, bad).unwrap_err();
            assert!(err.contains("dictionary"), "{bad}: {err}");
        }
        assert_eq!(base.dictionary, ["Keep"]);
    }

    #[test]
    fn a_dictionary_edit_flags_the_dictionary_and_formatting() {
        let base = Settings::default();
        let mut new = base.clone();
        new.dictionary = vec!["Shepit".into()];
        let c = changes(&base, &new);
        assert_eq!(
            c,
            Changes {
                dictionary: true,
                formatting: true,
                ..Changes::default()
            }
        );
        assert!(c.any());
        assert!(!changes(&new, &new).dictionary);
    }

    #[test]
    fn the_dictionary_round_trips_through_the_file() {
        let mut s = Settings::default();
        s.dictionary = vec!["Shepit".into(), "Vercel".into(), "Shepit".into()];
        let back = parse(&s.to_json());
        assert_eq!(back.dictionary, ["Shepit", "Vercel", "Shepit"]);
        assert_eq!(back, s);
    }

    #[test]
    fn a_stored_dictionary_keeps_order_and_duplicates() {
        let s = parse(r#"{"dictionary":["B","A","B"]}"#);
        assert_eq!(s.dictionary, ["B", "A", "B"]);
    }

    #[test]
    fn a_patch_with_one_bad_polishing_value_changes_nothing() {
        let base = Settings::default();
        assert!(patch_with(&base, r#"{"polishing":"off","claudeModel":"opus"}"#).is_err());
        assert_eq!(base.polishing, Polishing::Claude);
    }

    #[test]
    fn a_patch_may_keep_an_empty_speaker_note_and_a_cleared_path() {
        let mut base = Settings::default();
        base.claude_executable = "x.exe".into();
        let s = patch_with(&base, r#"{"speakerNotes":"","claudeExecutable":""}"#).unwrap();
        assert_eq!(s.speaker_notes, "");
        assert_eq!(s.claude_executable, "");
    }

    #[test]
    fn each_polishing_setting_flags_polishing_only() {
        let base = Settings::default();
        let edits: [fn(&mut Settings); 5] = [
            |s| s.polishing = Polishing::Off,
            |s| s.claude_model = ClaudeModel::Sonnet,
            |s| s.speaker_notes = "other".into(),
            |s| s.claude_executable = "a.exe".into(),
            |s| s.codex_executable = "b.exe".into(),
        ];
        for (i, edit) in edits.iter().enumerate() {
            let mut new = base.clone();
            edit(&mut new);
            let c = changes(&base, &new);
            assert_eq!(
                c,
                Changes {
                    polishing: true,
                    ..Changes::default()
                },
                "edit {i}"
            );
            assert!(c.any());
        }
    }

    #[test]
    fn each_formatting_setting_flags_formatting_only() {
        let base = Settings::default();
        // The dictionary also flags `dictionary`; its own test is above.
        let edits: [fn(&mut Settings); 3] = [
            |s| s.default_mode = Mode::Raw,
            |s| s.fast_path_max_words = 3,
            |s| s.prompt_mode_min_words = 60,
        ];
        for (i, edit) in edits.iter().enumerate() {
            let mut new = base.clone();
            edit(&mut new);
            let c = changes(&base, &new);
            assert_eq!(
                c,
                Changes {
                    formatting: true,
                    ..Changes::default()
                },
                "edit {i}"
            );
            assert!(c.any());
        }
    }

    #[test]
    fn polishing_and_formatting_flags_can_be_set_together() {
        let base = Settings::default();
        let mut new = base.clone();
        new.polishing = Polishing::Off;
        new.default_mode = Mode::Raw;
        let c = changes(&base, &new);
        assert!(c.polishing && c.formatting);
        assert!(!c.look && !c.hotkey && !c.visible);
    }

    // Update keys (docs/spec/settings.md, updates-install.md).

    #[test]
    fn the_update_defaults_match_the_spec() {
        let d = Settings::default();
        assert!(d.check_for_updates);
        assert_eq!(d.skipped_update, "");
        assert_eq!(d.last_update_check, None);
        assert_eq!(parse("{}"), d);
    }

    #[test]
    fn valid_update_values_are_read() {
        let s = parse(
            r#"{"checkForUpdates":false,"skippedUpdate":"0.4.0","lastUpdateCheck":1800000000}"#,
        );
        assert!(!s.check_for_updates);
        assert_eq!(s.skipped_update, "0.4.0");
        assert_eq!(s.last_update_check, Some(1_800_000_000));
    }

    #[test]
    fn a_wrong_check_for_updates_falls_back_alone() {
        for bad in ["\"no\"", "0", "null", "[]"] {
            let s = parse(&format!(
                r#"{{"checkForUpdates":{bad},"widgetVisible":false}}"#
            ));
            assert!(s.check_for_updates, "{bad}");
            assert!(!s.widget_visible, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn a_wrong_skipped_update_falls_back_alone() {
        for bad in ["5", "null", "[]"] {
            let s = parse(&format!(
                r#"{{"skippedUpdate":{bad},"widgetVisible":false}}"#
            ));
            assert_eq!(s.skipped_update, "", "{bad}");
            assert!(!s.widget_visible, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn a_wrong_last_update_check_falls_back_alone() {
        for bad in ["\"1800000000\"", "-5", "1.5", "null", "true", "{}"] {
            let s = parse(&format!(
                r#"{{"lastUpdateCheck":{bad},"widgetVisible":false}}"#
            ));
            assert_eq!(s.last_update_check, None, "{bad}");
            assert!(!s.widget_visible, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn the_update_keys_are_written_with_the_spec_names() {
        let s = Settings {
            check_for_updates: false,
            skipped_update: "0.4.0".into(),
            last_update_check: Some(1_800_000_000),
            ..Settings::default()
        };
        let out = written(&s);
        assert_eq!(out["checkForUpdates"], false);
        assert_eq!(out["skippedUpdate"], "0.4.0");
        assert_eq!(out["lastUpdateCheck"], 1_800_000_000u64);
        let d = written(&Settings::default());
        assert_eq!(d["checkForUpdates"], true);
        assert_eq!(d["skippedUpdate"], "");
    }

    #[test]
    fn an_unset_last_update_check_is_not_written() {
        assert!(written(&Settings::default())
            .get("lastUpdateCheck")
            .is_none());
    }

    #[test]
    fn the_update_keys_are_not_kept_as_extras() {
        let s = parse(
            r#"{"checkForUpdates":false,"skippedUpdate":"1","lastUpdateCheck":5,"engine":"x"}"#,
        );
        for key in ["checkForUpdates", "skippedUpdate", "lastUpdateCheck"] {
            assert!(!s.extra.contains_key(key), "{key}");
        }
        assert!(s.extra.contains_key("engine"));
    }

    #[test]
    fn write_then_parse_keeps_the_update_settings() {
        let s = Settings {
            check_for_updates: false,
            skipped_update: "0.4.0".into(),
            last_update_check: Some(1_800_000_000),
            ..Settings::default()
        };
        assert_eq!(parse(&s.to_json()), s);
    }

    #[test]
    fn a_patch_sets_check_for_updates() {
        let s = patch(&Settings::default(), r#"{"checkForUpdates":false}"#).unwrap();
        assert!(!s.check_for_updates);
    }

    #[test]
    fn a_non_boolean_check_for_updates_rejects_the_patch() {
        for bad in ["\"no\"", "0", "null", "[]"] {
            let r = patch(
                &Settings::default(),
                &format!(r#"{{"checkForUpdates":{bad},"widgetVisible":false}}"#),
            );
            assert!(r.is_err(), "{bad}");
        }
    }

    #[test]
    fn the_skipped_version_and_the_last_check_cannot_be_patched() {
        let skipped = patch(&Settings::default(), r#"{"skippedUpdate":"0.4.0"}"#);
        assert!(skipped.unwrap_err().contains("cannot be patched"));
        let last = patch(&Settings::default(), r#"{"lastUpdateCheck":5}"#);
        assert!(last.unwrap_err().contains("cannot be patched"));
    }

    #[test]
    fn check_for_updates_flags_updates_only() {
        let base = Settings::default();
        let new = Settings {
            check_for_updates: false,
            ..base.clone()
        };
        let c = changes(&base, &new);
        assert_eq!(
            c,
            Changes {
                updates: true,
                ..Changes::default()
            }
        );
        assert!(c.any());
    }

    #[test]
    fn the_skipped_version_and_the_last_check_flag_nothing() {
        let base = Settings::default();
        let new = Settings {
            skipped_update: "0.4.0".into(),
            last_update_check: Some(5),
            ..base.clone()
        };
        let c = changes(&base, &new);
        assert_eq!(c, Changes::default());
        assert!(!c.any());
    }

    // Live preview (docs/spec/settings.md: livePreview, default true).

    #[test]
    fn live_preview_defaults_to_on() {
        assert!(Settings::default().live_preview);
        assert!(parse("{}").live_preview);
    }

    #[test]
    fn live_preview_false_is_read() {
        assert!(!parse(r#"{"livePreview":false}"#).live_preview);
    }

    #[test]
    fn a_wrong_live_preview_falls_back_alone() {
        for bad in ["\"no\"", "0", "null", "[]"] {
            let s = parse(&format!(r#"{{"livePreview":{bad},"widgetVisible":false}}"#));
            assert!(s.live_preview, "{bad}");
            assert!(!s.widget_visible, "{bad} must not touch other keys");
        }
    }

    #[test]
    fn live_preview_is_written_as_live_preview() {
        let off = Settings {
            live_preview: false,
            ..Settings::default()
        };
        assert_eq!(written(&off)["livePreview"], false);
        assert_eq!(written(&Settings::default())["livePreview"], true);
    }

    #[test]
    fn live_preview_is_not_kept_as_an_extra() {
        let s = parse(r#"{"livePreview":false,"engine":"x"}"#);
        assert!(!s.extra.contains_key("livePreview"));
    }

    #[test]
    fn write_then_parse_keeps_live_preview() {
        let s = Settings {
            live_preview: false,
            ..Settings::default()
        };
        assert_eq!(parse(&s.to_json()), s);
    }

    #[test]
    fn a_patch_sets_live_preview() {
        let s = patch(&Settings::default(), r#"{"livePreview":false}"#).unwrap();
        assert!(!s.live_preview);
    }

    #[test]
    fn a_non_boolean_live_preview_rejects_the_patch() {
        for bad in ["\"yes\"", "0", "null", "[]"] {
            let r = patch(
                &Settings::default(),
                &format!(r#"{{"livePreview":{bad},"widgetVisible":false}}"#),
            );
            assert!(r.is_err(), "{bad}");
        }
    }

    #[test]
    fn live_preview_flags_live_preview_only() {
        let base = Settings::default();
        let new = Settings {
            live_preview: false,
            ..base.clone()
        };
        let c = changes(&base, &new);
        assert_eq!(
            c,
            Changes {
                live_preview: true,
                ..Changes::default()
            }
        );
        assert!(c.any());
    }
}
