//! The settings file of the Windows port: the widget keys of docs/spec/settings.md, read leniently
//! (each key falls back to its default on its own) and written with every other key kept.

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
const KNOWN_KEYS: [&str; 5] = [KEY_STYLE, KEY_OPACITY, KEY_FADE, KEY_VISIBLE, KEY_ANCHOR];

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
}
