use serde::Serialize;
use shepit_core::dictation::{Phase, Stage, View};

/// Logical window size for a view: the shape plus the shadow margin of 28 x 26 on each side, and
/// for a failed view the 440 px caption row (WidgetModel.swift:84, 87-102, 132-147).
pub fn window_size(view: &View) -> (f64, f64) {
    match (view.phase, view.hands_free) {
        (Phase::Idle, _) => (96.0, 60.0),
        (Phase::Listening, false) => (188.0, 84.0),
        (Phase::Listening, true) => (222.0, 84.0),
        (Phase::Processing | Phase::Done | Phase::Failed, _) => (520.0, 126.0),
    }
}

/// The view a fresh controller starts in; used to reset the widget after a controller restart.
pub fn idle_view() -> View {
    View {
        phase: Phase::Idle,
        hands_free: false,
        message: String::new(),
        levels: vec![0.0; 32],
        stage: None,
        progress: None,
    }
}

/// Whether the window must be resized and re-seated. Level-only updates arrive about 30 times a
/// second and must only emit the event.
pub fn needs_resize(old: &View, new: &View) -> bool {
    old.phase != new.phase || old.hands_free != new.hands_free
}

/// The `widget-state` event payload: {"phase","handsFree","message","levels","stage","progress"}.
/// `stage` and `progress` are always present and null when they do not apply.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetState {
    pub phase: String,
    pub hands_free: bool,
    pub message: String,
    pub levels: Vec<f32>,
    pub stage: Option<&'static str>,
    pub progress: Option<f32>,
}

impl WidgetState {
    pub fn from_view(view: &View) -> Self {
        let phase = match view.phase {
            Phase::Idle => "idle",
            Phase::Listening => "listening",
            Phase::Processing => "processing",
            Phase::Done => "done",
            Phase::Failed => "failed",
        };
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
        }
    }
}

/// Distance from the work area bottom to the idle pill's bottom edge, in logical points.
/// macOS: the anchor sits 4 pt above `visibleFrame.minY`, but `frame(for:)` clamps the panel
/// to `margin` (8 pt), so the panel bottom is 8 pt up; the pill is a further 26 pt up because
/// of the shadow margin (WidgetController.swift:27, 196-213; WidgetModel.swift:84).
const PILL_BOTTOM_ABOVE_WORK_AREA: f64 = 34.0;
/// Shadow margin below the pill inside the window, in logical points.
const SHADOW_MARGIN_BELOW: f64 = 26.0;
/// Minimum distance between the window and the work area edges (WidgetController.swift:27).
const EDGE_MARGIN: f64 = 8.0;

/// Top-left position for the widget window: centred horizontally, the idle pill's bottom edge
/// 34 logical points above the work area bottom, then clamped inside the work area like the
/// macOS panel. `work_area` is (x, y, width, height) and `window` is (width, height), both in
/// physical pixels; y grows downward.
pub fn default_position(
    work_area: (i32, i32, i32, i32),
    window: (i32, i32),
    scale: f64,
) -> (i32, i32) {
    let (wx, wy, w, h) = work_area;
    let margin = (EDGE_MARGIN * scale).round() as i32;
    let bottom_gap = ((PILL_BOTTOM_ABOVE_WORK_AREA - SHADOW_MARGIN_BELOW) * scale).round() as i32;
    let x = wx + w / 2 - window.0 / 2;
    let y = wy + h - bottom_gap - window.1;
    (
        x.min(wx + w - window.0 - margin).max(wx + margin),
        y.min(wy + h - window.1 - margin).max(wy + margin),
    )
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

    #[test]
    fn resize_only_when_phase_or_hands_free_changes() {
        let idle = view(Phase::Idle, false, "");
        let listening = view(Phase::Listening, false, "");
        let hands_free = view(Phase::Listening, true, "");
        let mut louder = listening.clone();
        louder.levels[31] = 0.9;
        let mut other_text = view(Phase::Failed, false, "a");
        assert!(needs_resize(&idle, &listening));
        assert!(needs_resize(&listening, &hands_free));
        assert!(!needs_resize(&listening, &louder));
        assert!(!needs_resize(&idle, &idle));
        let failed_b = view(Phase::Failed, false, "b");
        assert!(!needs_resize(&other_text, &failed_b));
        other_text.phase = Phase::Idle;
        assert!(needs_resize(&other_text, &failed_b));
    }

    #[test]
    fn idle_window_is_96_by_60() {
        assert_eq!(window_size(&view(Phase::Idle, false, "")), (96.0, 60.0));
    }

    #[test]
    fn push_to_talk_window_is_188_by_84() {
        assert_eq!(
            window_size(&view(Phase::Listening, false, "")),
            (188.0, 84.0)
        );
    }

    #[test]
    fn hands_free_window_is_222_by_84() {
        assert_eq!(
            window_size(&view(Phase::Listening, true, "")),
            (222.0, 84.0)
        );
    }

    #[test]
    fn failed_window_is_520_by_126() {
        assert_eq!(
            window_size(&view(Phase::Failed, false, "Microphone: access denied")),
            (520.0, 126.0)
        );
    }

    #[test]
    fn processing_and_done_windows_are_520_by_126() {
        assert_eq!(
            window_size(&view(Phase::Processing, false, "")),
            (520.0, 126.0)
        );
        assert_eq!(window_size(&view(Phase::Done, false, "hi")), (520.0, 126.0));
    }

    #[test]
    fn a_stage_or_progress_change_alone_does_not_resize() {
        let mut loading = view(Phase::Processing, false, "");
        loading.stage = Some(Stage::Loading);
        loading.progress = Some(0.3);
        let mut transcribing = loading.clone();
        transcribing.stage = Some(Stage::Transcribing);
        transcribing.progress = None;
        assert!(!needs_resize(&loading, &transcribing));
    }

    #[test]
    fn pill_stays_34_above_work_area_for_every_phase_at_scale_1() {
        for v in all_views() {
            let (w, h) = window_size(&v);
            let (_, y) = default_position((0, 0, 1920, 1040), (w as i32, h as i32), 1.0);
            let pill_bottom = y + h as i32 - SHADOW_MARGIN_BELOW as i32;
            assert_eq!(1040 - pill_bottom, 34, "{:?}", v.phase);
        }
    }

    #[test]
    fn pill_stays_34_logical_above_work_area_for_every_phase_at_scale_1_5() {
        for v in all_views() {
            let (w, h) = window_size(&v);
            let (pw, ph) = ((w * 1.5).round() as i32, (h * 1.5).round() as i32);
            let (_, y) = default_position((0, 0, 2560, 1400), (pw, ph), 1.5);
            let pill_bottom = y + ph - (SHADOW_MARGIN_BELOW * 1.5) as i32;
            assert_eq!(1400 - pill_bottom, 51, "{:?}", v.phase);
        }
    }

    #[test]
    fn widget_state_json_has_the_agreed_keys() {
        let mut v = view(Phase::Listening, true, "");
        v.levels[31] = 0.5;
        let json = serde_json::to_value(WidgetState::from_view(&v)).unwrap();
        let obj = json.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "handsFree",
                "levels",
                "message",
                "phase",
                "progress",
                "stage"
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
    }

    #[test]
    fn processing_payload_carries_stage_and_progress() {
        let mut v = view(Phase::Processing, false, "");
        v.stage = Some(Stage::Loading);
        v.progress = Some(0.5);
        let json = serde_json::to_value(WidgetState::from_view(&v)).unwrap();
        assert_eq!(json["phase"], "processing");
        assert_eq!(json["stage"], "loading");
        assert_eq!(json["progress"], 0.5);
        v.stage = Some(Stage::Transcribing);
        v.progress = None;
        let json = serde_json::to_value(WidgetState::from_view(&v)).unwrap();
        assert_eq!(json["stage"], "transcribing");
        assert!(json["progress"].is_null());
    }

    #[test]
    fn done_payload_carries_the_preview() {
        let json =
            serde_json::to_value(WidgetState::from_view(&view(Phase::Done, false, "hi"))).unwrap();
        assert_eq!(json["phase"], "done");
        assert_eq!(json["message"], "hi");
    }

    #[test]
    fn widget_state_phase_names_are_lowercase() {
        let phases: Vec<String> = all_views()
            .iter()
            .map(|v| WidgetState::from_view(v).phase)
            .collect();
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
    fn centres_on_primary_1080p() {
        // 96 x 60 window: bottom 8 above the work area, so the pill sits 34 above.
        assert_eq!(
            default_position((0, 0, 1920, 1040), (96, 60), 1.0),
            (912, 972)
        );
    }

    #[test]
    fn pill_bottom_is_34_logical_above_work_area_at_150_percent() {
        let (_, y) = default_position((0, 0, 2560, 1400), (144, 90), 1.5);
        let pill_bottom = y + 90 - (SHADOW_MARGIN_BELOW * 1.5) as i32;
        assert_eq!(1400 - pill_bottom, 51);
    }

    #[test]
    fn work_area_with_negative_origin() {
        assert_eq!(
            default_position((-1280, 0, 1280, 984), (96, 60), 1.0),
            (-688, 916)
        );
    }

    #[test]
    fn window_is_clamped_inside_the_work_area() {
        // Work area smaller than window plus margins: the 8 pt margin wins on the low side.
        assert_eq!(
            default_position((100, 50, 60, 70), (96, 60), 1.0),
            (108, 58)
        );
    }

    #[test]
    fn the_inserting_stage_reaches_the_payload() {
        let mut v = view(Phase::Processing, false, "");
        v.stage = Some(Stage::Inserting);
        assert_eq!(WidgetState::from_view(&v).stage, Some("inserting"));
    }
}
