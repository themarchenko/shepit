//! The settings file of the Windows port: the widget keys of docs/spec/settings.md, read leniently
//! (each key falls back to its default on its own) and written with every other key kept.

use crate::keys::Hotkey;
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
const KNOWN_KEYS: [&str; 7] = [
    KEY_STYLE,
    KEY_OPACITY,
    KEY_FADE,
    KEY_VISIBLE,
    KEY_ANCHOR,
    KEY_HOTKEY,
    KEY_ONBOARDING,
];

/// macOS does not clamp (AppSettings.swift:163); the Windows widget needs the glass readable
/// and never fully opaque, so a hand-edited value is pulled into the slider's range.
fn clamp_opacity(value: f64) -> f64 {
    value.clamp(MIN_GLASS_OPACITY, MAX_GLASS_OPACITY)
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
    /// Bottom centre of the widget, physical pixels. `None` means the default position.
    pub widget_anchor: Option<Point>,
    /// The dictation key. Persisted as `hotkey`: "rightCtrl", "rightAlt", "f13" to "f24".
    pub hotkey: Hotkey,
    /// Set once the welcome tour was finished, skipped or closed. Persisted as
    /// `onboardingCompleted`.
    pub onboarding_completed: bool,
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
            widget_anchor: None,
            hotkey: Hotkey::RightCtrl,
            onboarding_completed: false,
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
        map.insert(KEY_HOTKEY.into(), Value::from(self.hotkey.id()));
        map.insert(
            KEY_ONBOARDING.into(),
            Value::from(self.onboarding_completed),
        );
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
}

impl Changes {
    pub fn any(&self) -> bool {
        self.look || self.hotkey || self.visible
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
    }
}

/// Applies a UI edit. The patch is a JSON object with any of `widgetStyle`, `glassOpacity`,
/// `fadeWhenIdle`, `widgetVisible` and `hotkey`. An unknown key, a wrong type or a bad id rejects
/// the whole patch and leaves `current` as it was. `glassOpacity` is clamped like a loaded file.
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
            KEY_HOTKEY => {
                out.hotkey = value
                    .as_str()
                    .and_then(Hotkey::from_id)
                    .ok_or_else(|| format!("{KEY_HOTKEY}: unknown key"))?;
            }
            // widgetAnchor and onboardingCompleted have their own commands.
            other => return Err(format!("unknown setting: {other}")),
        }
    }
    Ok(out)
}

#[cfg(test)]
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
                visible: true
            }
        );
    }
}
