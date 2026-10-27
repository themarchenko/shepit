use serde::Serialize;
use shepit_core::dictation::{Phase, Stage, View};
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
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            style: Style::FlowPill,
            glass_opacity: 0.7,
            fade_when_idle: true,
            hovering: false,
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
            key_label: KEY_LABEL.to_string(),
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

/// The hotkey's name as the widget shows it. Fixed until the hotkey can be chosen.
const KEY_LABEL: &str = "Right Ctrl";

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

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::dictation::Stage;

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
