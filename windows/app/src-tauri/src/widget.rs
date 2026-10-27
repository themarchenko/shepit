use serde::Serialize;
use shepit_core::dictation::{Phase, Stage, View};
use shepit_core::keys::Hotkey;
use shepit_core::widget::{
    frame_for, layout_for, monitor_for, LayoutInput, Monitor, Point, Rect, Size, Style,
};

/// The view a fresh controller starts in; used to reset the widget after a controller restart.
pub fn idle_view() -> View {
    View {
        phase: Phase::Idle,
        hands_free: false,
        message: String::new(),
        levels: vec![0.0; 32],
        stage: None,
        progress: None,
        target: None,
    }
}

/// What the payload needs besides the view: the settings and the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Presentation {
    pub style: Style,
    pub glass_opacity: f64,
    pub fade_when_idle: bool,
    pub hovering: bool,
    /// Its label is the `keyLabel` of the payload.
    pub hotkey: Hotkey,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            style: Style::FlowPill,
            glass_opacity: 0.7,
            fade_when_idle: true,
            hovering: false,
            hotkey: Hotkey::RightCtrl,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveJson {
    pub stable: String,
    pub unstable: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShapeJson {
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowJson {
    pub width: f64,
    pub height: f64,
}

/// The `widget-state` event payload: {"phase","handsFree","message","levels","stage","progress",
/// "style","glassOpacity","fadeWhenIdle","keyLabel","hovering","target","live","shape","caption",
/// "window"}. `stage`, `progress` and `target` are always present and null when they do not apply.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetState {
    pub phase: String,
    pub hands_free: bool,
    pub message: String,
    pub levels: Vec<f32>,
    pub stage: Option<&'static str>,
    pub progress: Option<f32>,
    pub style: String,
    pub glass_opacity: f64,
    pub fade_when_idle: bool,
    pub key_label: String,
    pub hovering: bool,
    pub target: Option<String>,
    pub live: LiveJson,
    pub shape: ShapeJson,
    pub caption: bool,
    pub window: WindowJson,
}

impl WidgetState {
    /// Fills the layout numbers from the core table, so the frontend never keeps its own copy.
    pub fn new(view: &View, presentation: &Presentation) -> Self {
        let phase = match view.phase {
            Phase::Idle => "idle",
            Phase::Listening => "listening",
            Phase::Processing => "processing",
            Phase::Done => "done",
            Phase::Failed => "failed",
        };
        let layout = layout_for(&layout_input(view, presentation));
        Self {
            phase: phase.to_string(),
            hands_free: view.hands_free,
            message: view.message.clone(),
            levels: view.levels.clone(),
            stage: view.stage.map(|s| match s {
                Stage::Loading => "loading",
                Stage::Transcribing => "transcribing",
                Stage::Inserting => "inserting",
            }),
            progress: view.progress,
            style: presentation.style.id().to_string(),
            glass_opacity: presentation.glass_opacity,
            fade_when_idle: presentation.fade_when_idle,
            key_label: presentation.hotkey.label().to_string(),
            hovering: presentation.hovering,
            target: view.target.clone(),
            // Stays empty until the live preview exists (task 12).
            live: LiveJson {
                stable: String::new(),
                unstable: String::new(),
            },
            shape: ShapeJson {
                width: layout.shape.width,
                height: layout.shape.height,
                radius: layout.shape.radius,
            },
            caption: layout.caption,
            window: WindowJson {
                width: layout.window.width,
                height: layout.window.height,
            },
        }
    }
}

/// One step of the Settings demo: the widget state to show and how long to show it.
#[derive(Debug, Clone, Serialize)]
pub struct DemoStep {
    pub state: WidgetState,
    pub seconds: f64,
}

/// The demo loop of Settings and the tour (WidgetDemo.swift:41-72 without the formatting stage):
/// idle 1.6 s, hover 1.3 s, listening 4.1 s, transcribing 0.75 s, inserting 0.4 s, done 2.6 s.
/// Built with the real layout, fade off, `hotkey`'s label on the keycap.
pub fn demo_script(style: Style, hotkey: Hotkey) -> Vec<DemoStep> {
    let presentation = |hovering: bool| Presentation {
        style,
        fade_when_idle: false,
        hovering,
        hotkey,
        ..Presentation::default()
    };
    let view = |phase: Phase, stage: Option<Stage>, message: &str| View {
        phase,
        hands_free: false,
        message: message.to_string(),
        // The frontend animates the bars; the script only fixes the shape.
        levels: vec![0.0; 32],
        stage,
        progress: None,
        target: (phase != Phase::Idle).then(|| DEMO_TARGET.to_string()),
    };
    let step = |view: View, hovering: bool, seconds: f64| DemoStep {
        state: WidgetState::new(&view, &presentation(hovering)),
        seconds,
    };
    vec![
        step(view(Phase::Idle, None, ""), false, 1.6),
        step(view(Phase::Idle, None, ""), true, 1.3),
        step(view(Phase::Listening, None, ""), false, 4.1),
        step(
            view(Phase::Processing, Some(Stage::Transcribing), ""),
            false,
            0.75,
        ),
        step(
            view(Phase::Processing, Some(Stage::Inserting), ""),
            false,
            0.4,
        ),
        step(view(Phase::Done, None, DEMO_TEXT), false, 2.6),
    ]
}

const DEMO_TARGET: &str = "Notepad";
const DEMO_TEXT: &str = "Зустрінемось завтра о десятій, I'll send the notes.";

fn layout_input(view: &View, presentation: &Presentation) -> LayoutInput {
    LayoutInput {
        style: presentation.style,
        phase: view.phase,
        hands_free: view.hands_free,
        hovering: presentation.hovering,
        has_live: false,
    }
}

/// The window size (logical) the view needs.
pub fn window_for(view: &View, presentation: &Presentation) -> Size {
    layout_for(&layout_input(view, presentation)).window
}

/// Logical size to physical pixels at `scale`.
pub fn physical_size(size: Size, scale: f64) -> (i32, i32) {
    (
        (size.width * scale).round() as i32,
        (size.height * scale).round() as i32,
    )
}

/// The window frame for `size` around `anchor`, on the monitor that holds the anchor (the primary
/// when none does). `None` without monitors. `monitors[0]` is the primary.
pub fn placement(anchor: Point, size: Size, monitors: &[Monitor]) -> Option<Rect> {
    let monitor = monitors.get(monitor_for(anchor, monitors).unwrap_or(0))?;
    Some(frame_for(
        anchor,
        physical_size(size, monitor.scale),
        monitor.work,
        monitor.scale,
    ))
}

/// The bottom centre of the window as `placement` really puts it, so a stored anchor never sits
/// outside the work area. Unchanged when there is no monitor.
pub fn clamped_anchor(anchor: Point, size: Size, monitors: &[Monitor]) -> Point {
    match placement(anchor, size, monitors) {
        Some(frame) => Point {
            x: frame.x + frame.width / 2,
            y: frame.y + frame.height,
        },
        None => anchor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::dictation::Stage;

    #[test]
    fn the_stored_anchor_is_the_clamped_one() {
        let size = Size {
            width: 96.0,
            height: 60.0,
        };
        // Dropped with the window half outside the right edge of the 1920 x 1080 work area.
        let raw = Point { x: 1915, y: 1032 };
        let kept = clamped_anchor(raw, size, &monitors());
        assert_eq!(
            kept,
            Point {
                x: 1920 - 8 - 48,
                y: 1032
            }
        );
        // Already inside: unchanged.
        let inside = Point { x: 960, y: 1032 };
        assert_eq!(clamped_anchor(inside, size, &monitors()), inside);
    }

    fn view(phase: Phase, hands_free: bool, message: &str) -> View {
        View {
            phase,
            hands_free,
            message: message.to_string(),
            levels: vec![0.0; 32],
            stage: None,
            progress: None,
            target: None,
        }
    }

    fn all_views() -> Vec<View> {
        vec![
            view(Phase::Idle, false, ""),
            view(Phase::Listening, false, ""),
            view(Phase::Listening, true, ""),
            view(Phase::Processing, false, ""),
            view(Phase::Done, false, "hello"),
            view(Phase::Failed, false, "Microphone: no microphone found"),
        ]
    }

    fn state(v: &View) -> WidgetState {
        WidgetState::new(v, &Presentation::default())
    }

    fn payload(v: &View, p: &Presentation) -> serde_json::Value {
        serde_json::to_value(WidgetState::new(v, p)).unwrap()
    }

    #[test]
    fn widget_state_json_has_the_agreed_keys() {
        let mut v = view(Phase::Listening, true, "");
        v.levels[31] = 0.5;
        let json = payload(&v, &Presentation::default());
        let obj = json.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "caption",
                "fadeWhenIdle",
                "glassOpacity",
                "handsFree",
                "hovering",
                "keyLabel",
                "levels",
                "live",
                "message",
                "phase",
                "progress",
                "shape",
                "stage",
                "style",
                "target",
                "window"
            ]
        );
        assert_eq!(obj["phase"], "listening");
        assert_eq!(obj["handsFree"], true);
        assert_eq!(obj["message"], "");
        assert_eq!(obj["levels"].as_array().unwrap().len(), 32);
        assert_eq!(obj["levels"][31], 0.5);
        // Present as null so the frontend never sees a missing key.
        assert!(obj["progress"].is_null());
        assert!(obj["stage"].is_null());
        assert!(obj["target"].is_null());
    }

    #[test]
    fn the_payload_carries_the_presentation() {
        let p = Presentation {
            style: Style::Island,
            glass_opacity: 0.55,
            fade_when_idle: false,
            hovering: true,
            hotkey: Hotkey::RightCtrl,
        };
        let json = payload(&view(Phase::Idle, false, ""), &p);
        assert_eq!(json["style"], "island");
        assert_eq!(json["glassOpacity"], 0.55);
        assert_eq!(json["fadeWhenIdle"], false);
        assert_eq!(json["hovering"], true);
        assert_eq!(json["keyLabel"], "Right Ctrl");
    }

    #[test]
    fn the_payload_carries_the_layout_numbers() {
        // flowPill, hands-free: 166 x 32 capsule in a 222 x 84 window (WidgetModel.swift:87-92, 146).
        let json = payload(&view(Phase::Listening, true, ""), &Presentation::default());
        assert_eq!(json["shape"]["width"], 166.0);
        assert_eq!(json["shape"]["height"], 32.0);
        assert_eq!(json["shape"]["radius"], 16.0);
        assert_eq!(json["caption"], false);
        assert_eq!(json["window"]["width"], 222.0);
        assert_eq!(json["window"]["height"], 84.0);
    }

    #[test]
    fn a_failed_flow_pill_payload_has_the_caption_row_and_window() {
        let json = payload(
            &view(Phase::Failed, false, "Microphone: no microphone found"),
            &Presentation::default(),
        );
        assert_eq!(json["caption"], true);
        assert_eq!(json["window"]["width"], 520.0);
        assert_eq!(json["window"]["height"], 126.0);
    }

    #[test]
    fn a_hovered_idle_island_payload_is_280_by_30() {
        let p = Presentation {
            style: Style::Island,
            hovering: true,
            ..Presentation::default()
        };
        let json = payload(&view(Phase::Idle, false, ""), &p);
        assert_eq!(json["shape"]["width"], 280.0);
        assert_eq!(json["shape"]["height"], 30.0);
        assert_eq!(json["shape"]["radius"], 15.0);
        assert_eq!(json["window"]["width"], 336.0);
        assert_eq!(json["window"]["height"], 82.0);
    }

    #[test]
    fn the_target_app_name_reaches_the_payload() {
        let mut v = view(Phase::Listening, false, "");
        v.target = Some("Notepad".into());
        assert_eq!(payload(&v, &Presentation::default())["target"], "Notepad");
    }

    #[test]
    fn live_text_is_empty_until_task_12() {
        let json = payload(&view(Phase::Listening, false, ""), &Presentation::default());
        assert_eq!(json["live"]["stable"], "");
        assert_eq!(json["live"]["unstable"], "");
    }

    #[test]
    fn processing_payload_carries_stage_and_progress() {
        let mut v = view(Phase::Processing, false, "");
        v.stage = Some(Stage::Loading);
        v.progress = Some(0.5);
        let json = serde_json::to_value(state(&v)).unwrap();
        assert_eq!(json["phase"], "processing");
        assert_eq!(json["stage"], "loading");
        assert_eq!(json["progress"], 0.5);
        v.stage = Some(Stage::Transcribing);
        v.progress = None;
        let json = serde_json::to_value(state(&v)).unwrap();
        assert_eq!(json["stage"], "transcribing");
        assert!(json["progress"].is_null());
    }

    #[test]
    fn done_payload_carries_the_preview() {
        let json = serde_json::to_value(state(&view(Phase::Done, false, "hi"))).unwrap();
        assert_eq!(json["phase"], "done");
        assert_eq!(json["message"], "hi");
    }

    #[test]
    fn widget_state_phase_names_are_lowercase() {
        let phases: Vec<String> = all_views().iter().map(|v| state(v).phase).collect();
        assert_eq!(
            phases,
            [
                "idle",
                "listening",
                "listening",
                "processing",
                "done",
                "failed"
            ]
        );
    }

    #[test]
    fn the_inserting_stage_reaches_the_payload() {
        let mut v = view(Phase::Processing, false, "");
        v.stage = Some(Stage::Inserting);
        assert_eq!(state(&v).stage, Some("inserting"));
    }

    #[test]
    fn the_key_label_follows_the_hotkey() {
        for (hotkey, label) in [
            (Hotkey::RightCtrl, "Right Ctrl"),
            (Hotkey::RightAlt, "Right Alt"),
            (Hotkey::F13, "F13"),
            (Hotkey::F24, "F24"),
        ] {
            let p = Presentation {
                hotkey,
                ..Presentation::default()
            };
            let json = payload(&view(Phase::Idle, false, ""), &p);
            assert_eq!(json["keyLabel"], label);
        }
    }

    // The demo.

    fn script(style: Style) -> Vec<DemoStep> {
        demo_script(style, Hotkey::RightCtrl)
    }

    #[test]
    fn the_demo_has_six_steps_in_order() {
        let steps = script(Style::FlowPill);
        let seen: Vec<(String, Option<&str>, bool)> = steps
            .iter()
            .map(|s| (s.state.phase.clone(), s.state.stage, s.state.hovering))
            .collect();
        assert_eq!(
            seen,
            [
                ("idle".to_string(), None, false),
                ("idle".to_string(), None, true),
                ("listening".to_string(), None, false),
                ("processing".to_string(), Some("transcribing"), false),
                ("processing".to_string(), Some("inserting"), false),
                ("done".to_string(), None, false),
            ]
        );
    }

    #[test]
    fn the_demo_timings_follow_the_mac_demo() {
        let seconds: Vec<f64> = script(Style::Island).iter().map(|s| s.seconds).collect();
        assert_eq!(seconds, [1.6, 1.3, 4.1, 0.75, 0.4, 2.6]);
    }

    #[test]
    fn the_demo_keeps_the_widget_on_screen() {
        for style in [Style::FlowPill, Style::CaptionBar, Style::Island] {
            for step in script(style) {
                assert!(!step.state.fade_when_idle, "{style:?}");
            }
        }
    }

    #[test]
    fn the_demo_uses_the_style_it_was_asked_for() {
        for style in [Style::FlowPill, Style::CaptionBar, Style::Island] {
            for step in script(style) {
                assert_eq!(step.state.style, style.id());
            }
        }
    }

    #[test]
    fn the_demo_passes_the_key_label_through() {
        for step in demo_script(Style::FlowPill, Hotkey::RightAlt) {
            assert_eq!(step.state.key_label, "Right Alt");
        }
        for step in demo_script(Style::FlowPill, Hotkey::F13) {
            assert_eq!(step.state.key_label, "F13");
        }
    }

    #[test]
    fn the_demo_steps_carry_the_real_layout() {
        for style in [Style::FlowPill, Style::CaptionBar, Style::Island] {
            for step in script(style) {
                let want = layout_for(&LayoutInput {
                    style,
                    phase: match step.state.phase.as_str() {
                        "idle" => Phase::Idle,
                        "listening" => Phase::Listening,
                        "processing" => Phase::Processing,
                        "done" => Phase::Done,
                        other => panic!("phase {other}"),
                    },
                    hands_free: step.state.hands_free,
                    hovering: step.state.hovering,
                    has_live: false,
                });
                assert_eq!(step.state.shape.width, want.shape.width, "{style:?}");
                assert_eq!(step.state.shape.height, want.shape.height, "{style:?}");
                assert_eq!(step.state.window.width, want.window.width, "{style:?}");
                assert_eq!(step.state.window.height, want.window.height, "{style:?}");
                assert_eq!(step.state.caption, want.caption, "{style:?}");
            }
        }
    }

    #[test]
    fn the_demo_dictates_into_notepad_and_shows_the_text_when_done() {
        let steps = script(Style::FlowPill);
        for step in &steps[2..] {
            assert_eq!(step.state.target.as_deref(), Some("Notepad"));
        }
        assert_eq!(
            steps[5].state.message,
            "Зустрінемось завтра о десятій, I'll send the notes."
        );
    }

    #[test]
    fn the_demo_listens_push_to_talk_with_no_live_text() {
        let listening = &script(Style::FlowPill)[2].state;
        assert!(!listening.hands_free);
        assert_eq!(listening.live.stable, "");
        assert_eq!(listening.live.unstable, "");
    }

    #[test]
    fn a_demo_step_serializes_as_state_and_seconds() {
        let json = serde_json::to_value(&script(Style::FlowPill)[0]).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(keys, ["seconds", "state"]);
        assert_eq!(json["seconds"], 1.6);
        assert_eq!(json["state"]["phase"], "idle");
    }

    fn monitors() -> Vec<Monitor> {
        vec![
            Monitor {
                work: Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1040,
                },
                scale: 1.0,
            },
            Monitor {
                work: Rect {
                    x: 1920,
                    y: 0,
                    width: 2560,
                    height: 1400,
                },
                scale: 1.5,
            },
        ]
    }

    #[test]
    fn placement_scales_the_window_by_the_monitor_that_holds_the_anchor() {
        let size = Size {
            width: 96.0,
            height: 60.0,
        };
        let first = placement(Point { x: 960, y: 1032 }, size, &monitors()).unwrap();
        assert_eq!((first.width, first.height), (96, 60));
        let second = placement(Point { x: 3200, y: 1392 }, size, &monitors()).unwrap();
        assert_eq!((second.width, second.height), (144, 90));
    }

    #[test]
    fn placement_falls_back_to_the_primary_and_needs_a_monitor() {
        let size = Size {
            width: 96.0,
            height: 60.0,
        };
        let far = placement(Point { x: -5000, y: 10 }, size, &monitors()).unwrap();
        assert_eq!((far.x, far.y), (8, 8));
        assert_eq!(placement(Point { x: 0, y: 0 }, size, &[]), None);
    }
}
