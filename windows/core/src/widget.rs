//! Widget geometry and pointer input for the Windows port: shape sizes per style and phase
//! (WidgetModel.swift:78-147), the grow-now/shrink-later window fit (WidgetController.swift:158-170),
//! anchor clamping and monitor matching (:192-218), and the click/hold/drag gesture
//! (WidgetPanel.swift:52-95).

use crate::dictation::Phase;

pub const MARGIN_WIDTH: f64 = 28.0;
pub const MARGIN_HEIGHT: f64 = 26.0;
/// Seconds a press must last before it becomes push-to-talk (WidgetPanel.swift:59).
pub const HOLD_SECS: f64 = 0.35;
/// Logical pixels the pointer must travel before a press becomes a drag (WidgetPanel.swift:74).
pub const DRAG_THRESHOLD: f64 = 4.0;
/// Seconds the window waits before it shrinks (WidgetController.swift:166).
pub const SHRINK_DELAY_SECS: f64 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    FlowPill,
    CaptionBar,
    Island,
}

impl Style {
    /// The persisted id: "flowPill", "captionBar", "island".
    pub fn id(self) -> &'static str {
        match self {
            Style::FlowPill => "flowPill",
            Style::CaptionBar => "captionBar",
            Style::Island => "island",
        }
    }

    pub fn from_id(id: &str) -> Option<Style> {
        match id {
            "flowPill" => Some(Style::FlowPill),
            "captionBar" => Some(Style::CaptionBar),
            "island" => Some(Style::Island),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutInput {
    pub style: Style,
    pub phase: Phase,
    pub hands_free: bool,
    pub hovering: bool,
    pub has_live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub shape: Shape,
    /// True when the flow pill shows its caption row above the shape. Always false for the
    /// other styles, which draw their text inside the shape.
    pub caption: bool,
    /// The logical window size: the shape (plus the caption row) and the shadow margin.
    pub window: Size,
}

/// Width of the flow pill's caption row, plus its 12 pt side room (WidgetModel.swift:79, 121).
const CAPTION_WIDTH: f64 = 440.0 + 24.0;
/// Caption row height and the gap above the pill (WidgetModel.swift:127).
const CAPTION_ROW: f64 = 8.0 + 34.0;

/// Hover hint widths. macOS has 262, 330 and 252, which fit its short "Right ⌥" key label. The hint
/// here names "Right Ctrl" in a 11 px monospace keycap next to Segoe UI 12 px text, which is wider, so the
/// Windows shapes are wider (docs/spec/widget.md). The frontend shrinks the text to 10 px as a last resort.
const FLOW_PILL_HOVER_WIDTH: f64 = 300.0;
const CAPTION_BAR_HOVER_WIDTH: f64 = 360.0;
const ISLAND_HOVER_WIDTH: f64 = 280.0;
/// macOS has 96: mic icon, 8 pt gap and the "Right \u{2325}" keycap inside 14 pt padding. "Right Ctrl" is
/// longer, so the idle island is 128 here. An estimate for Segoe UI and Consolas until a device check.
const ISLAND_IDLE_WIDTH: f64 = 128.0;

pub fn layout_for(input: &LayoutInput) -> Layout {
    // Hover only grows an idle widget, and only the flow pill and the island read hands-free.
    let idle = input.phase == Phase::Idle;
    let hovering = idle && input.hovering;
    let capsule = |width: f64, height: f64| Shape {
        width,
        height,
        radius: height / 2.0,
    };
    let (shape, caption) = match input.style {
        Style::FlowPill => {
            let (w, h) = match input.phase {
                Phase::Idle if hovering => (FLOW_PILL_HOVER_WIDTH, 28.0),
                Phase::Idle => (40.0, 8.0),
                Phase::Listening => (if input.hands_free { 166.0 } else { 132.0 }, 32.0),
                Phase::Processing => (112.0, 32.0),
                Phase::Done | Phase::Failed => (44.0, 32.0),
            };
            let caption = match input.phase {
                Phase::Idle => false,
                Phase::Listening => input.has_live,
                Phase::Processing | Phase::Done | Phase::Failed => true,
            };
            (capsule(w, h), caption)
        }
        Style::CaptionBar => {
            let (w, h) = match input.phase {
                Phase::Idle => (
                    if hovering {
                        CAPTION_BAR_HOVER_WIDTH
                    } else {
                        178.0
                    },
                    30.0,
                ),
                Phase::Failed => (270.0, 42.0),
                _ => (560.0, 42.0),
            };
            (capsule(w, h), false)
        }
        Style::Island => {
            let (w, h, radius) = match input.phase {
                Phase::Idle if hovering => (ISLAND_HOVER_WIDTH, 30.0, 15.0),
                Phase::Idle => (ISLAND_IDLE_WIDTH, 26.0, 13.0),
                Phase::Listening if input.has_live => (410.0, 94.0, 24.0),
                Phase::Listening => (270.0, 36.0, 18.0),
                Phase::Processing => (410.0, 122.0, 26.0),
                Phase::Done => (410.0, 94.0, 24.0),
                Phase::Failed => (254.0, 36.0, 18.0),
            };
            (
                Shape {
                    width: w,
                    height: h,
                    radius,
                },
                false,
            )
        }
    };
    let mut width = shape.width;
    let mut height = shape.height;
    if caption {
        width = width.max(CAPTION_WIDTH);
        height += CAPTION_ROW;
    }
    Layout {
        shape,
        caption,
        window: Size {
            width: (width + MARGIN_WIDTH * 2.0).ceil(),
            height: (height + MARGIN_HEIGHT * 2.0).ceil(),
        },
    }
}

/// Physical pixels, virtual-screen coordinates, y down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Points kept between the window and the work area edge (WidgetController.swift:211-212).
const CLAMP_MARGIN: f64 = 8.0;
/// Points a saved anchor may sit outside a work area and still match it (:218).
const SLACK_X: f64 = 2.0;
const SLACK_Y: f64 = 16.0;

/// A monitor's work area in physical pixels and its scale factor (1.0 = 100%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    pub work: Rect,
    pub scale: f64,
}

/// The window frame for `size` (physical pixels) around `anchor`, the bottom centre of the window,
/// kept inside `work` with an 8 pt margin (`8 * scale` pixels, rounded). When the work area is too
/// small for the window the low edge wins, as in the old `default_position`.
pub fn frame_for(anchor: Point, size: (i32, i32), work: Rect, scale: f64) -> Rect {
    let (width, height) = size;
    let margin = (CLAMP_MARGIN * scale).round() as i32;
    // max(min()) so the low edge wins when the window does not fit.
    let x = (anchor.x - width / 2).min(work.x + work.width - width - margin);
    let y = (anchor.y - height).min(work.y + work.height - height - margin);
    Rect {
        x: x.max(work.x + margin),
        y: y.max(work.y + margin),
        width,
        height,
    }
}

/// Index of the first monitor whose work area, grown by 2 pt left and right and 16 pt up and down
/// (scaled by that monitor's factor), contains `anchor`.
pub fn monitor_for(anchor: Point, monitors: &[Monitor]) -> Option<usize> {
    let (x, y) = (f64::from(anchor.x), f64::from(anchor.y));
    monitors.iter().position(|m| {
        let (sx, sy) = (SLACK_X * m.scale, SLACK_Y * m.scale);
        let left = f64::from(m.work.x) - sx;
        let top = f64::from(m.work.y) - sy;
        let right = f64::from(m.work.x + m.work.width) + sx;
        let bottom = f64::from(m.work.y + m.work.height) + sy;
        // Min-inclusive, max-exclusive, like NSRect.contains.
        x >= left && x < right && y >= top && y < bottom
    })
}

/// Bottom centre of the monitor's work area, `8 * scale` pixels up.
pub fn default_anchor(monitor: &Monitor) -> Point {
    let w = monitor.work;
    Point {
        x: w.x + w.width / 2,
        y: w.y + w.height - (CLAMP_MARGIN * monitor.scale).round() as i32,
    }
}

/// `saved` when a monitor holds it, else the default anchor of `monitors[0]`, the primary.
pub fn resolve_anchor(saved: Option<Point>, monitors: &[Monitor]) -> Point {
    match saved {
        Some(p) if monitor_for(p, monitors).is_some() => p,
        _ => monitors
            .first()
            .map_or(Point { x: 0, y: 0 }, default_anchor),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shrink {
    pub size: Size,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// The window size to apply at once: the union of the current size and the target.
    pub now: Size,
    /// Set when the target is smaller than `now` in some dimension.
    pub later: Option<Shrink>,
}

/// Window sizing: grow at once, shrink after the shape's spring has settled.
#[derive(Debug, Clone)]
pub struct Fitter {
    current: Size,
    generation: u64,
    /// The size to shrink to, valid for `generation` only.
    pending: Option<Size>,
}

impl Fitter {
    pub fn new(initial: Size) -> Self {
        Self {
            current: initial,
            generation: 0,
            pending: None,
        }
    }

    pub fn current(&self) -> Size {
        self.current
    }

    pub fn request(&mut self, target: Size) -> Fit {
        // Every request invalidates the shrink still waiting from an earlier one.
        self.generation += 1;
        let now = Size {
            width: self.current.width.max(target.width),
            height: self.current.height.max(target.height),
        };
        self.current = now;
        if target == now {
            self.pending = None;
            return Fit { now, later: None };
        }
        self.pending = Some(target);
        Fit {
            now,
            later: Some(Shrink {
                size: target,
                generation: self.generation,
            }),
        }
    }

    /// The size to shrink to when `generation` is still the latest request, else `None`.
    pub fn shrink_due(&mut self, generation: u64) -> Option<Size> {
        if generation != self.generation {
            return None;
        }
        let size = self.pending.take()?;
        self.current = size;
        Some(size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerOut {
    None,
    Click,
    HoldBegan,
    HoldEnded,
    /// Move the window by this much (logical pixels since the last drag step).
    Dragged {
        dx: f64,
        dy: f64,
    },
    DragEnded,
}

/// Click, hold and drag from pointer events. `tick` stands in for the 0.35 s timer.
#[derive(Debug, Clone)]
pub struct PointerGesture {
    scale: f64,
    press: Option<Press>,
}

#[derive(Debug, Clone, Copy)]
struct Press {
    at: f64,
    origin: (f64, f64),
    last: (f64, f64),
    mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Down, not yet a hold or a drag.
    Pending,
    Hold,
    Drag,
}

impl PointerGesture {
    /// `scale` converts the 4 pt drag threshold to the coordinates the caller passes.
    pub fn new(scale: f64) -> Self {
        Self { scale, press: None }
    }

    pub fn down(&mut self, at: f64, x: f64, y: f64) -> PointerOut {
        self.press = Some(Press {
            at,
            origin: (x, y),
            last: (x, y),
            mode: Mode::Pending,
        });
        PointerOut::None
    }

    pub fn moved(&mut self, x: f64, y: f64) -> PointerOut {
        let threshold = DRAG_THRESHOLD * self.scale;
        let Some(press) = self.press.as_mut() else {
            return PointerOut::None;
        };
        match press.mode {
            // A hold keeps the widget still: the user is talking, not moving it.
            Mode::Hold => return PointerOut::None,
            Mode::Pending => {
                let travelled = (x - press.origin.0).hypot(y - press.origin.1);
                if travelled < threshold {
                    return PointerOut::None;
                }
                // The first step carries the whole distance so the window catches up.
                press.mode = Mode::Drag;
                press.last = press.origin;
            }
            Mode::Drag => {}
        }
        let out = PointerOut::Dragged {
            dx: x - press.last.0,
            dy: y - press.last.1,
        };
        press.last = (x, y);
        out
    }

    pub fn tick(&mut self, now: f64) -> PointerOut {
        match self.press.as_mut() {
            Some(p) if p.mode == Mode::Pending && now - p.at >= HOLD_SECS => {
                p.mode = Mode::Hold;
                PointerOut::HoldBegan
            }
            _ => PointerOut::None,
        }
    }

    pub fn up(&mut self, _at: f64) -> PointerOut {
        match self.press.take().map(|p| p.mode) {
            Some(Mode::Pending) => PointerOut::Click,
            Some(Mode::Hold) => PointerOut::HoldEnded,
            Some(Mode::Drag) => PointerOut::DragEnded,
            None => PointerOut::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(style: Style, phase: Phase) -> LayoutInput {
        LayoutInput {
            style,
            phase,
            hands_free: false,
            hovering: false,
            has_live: false,
        }
    }

    fn sz(width: f64, height: f64) -> Size {
        Size { width, height }
    }

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn pt(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn mon(x: i32, y: i32, width: i32, height: i32, scale: f64) -> Monitor {
        Monitor {
            work: rect(x, y, width, height),
            scale,
        }
    }

    // Style ids.

    #[test]
    fn style_ids_round_trip() {
        for s in [Style::FlowPill, Style::CaptionBar, Style::Island] {
            assert_eq!(Style::from_id(s.id()), Some(s));
        }
        assert_eq!(Style::FlowPill.id(), "flowPill");
        assert_eq!(Style::CaptionBar.id(), "captionBar");
        assert_eq!(Style::Island.id(), "island");
    }

    #[test]
    fn an_unknown_style_id_is_none() {
        assert_eq!(Style::from_id("pill"), None);
        assert_eq!(Style::from_id("FlowPill"), None);
        assert_eq!(Style::from_id(""), None);
    }

    // Layout: the 24-case table is in the fixture; these pin the rules behind it.

    #[test]
    fn hover_only_matters_while_idle() {
        let mut i = input(Style::FlowPill, Phase::Listening);
        let calm = layout_for(&i);
        i.hovering = true;
        assert_eq!(layout_for(&i), calm);
    }

    #[test]
    fn island_ignores_hands_free() {
        let mut i = input(Style::Island, Phase::Listening);
        let ptt = layout_for(&i);
        i.hands_free = true;
        assert_eq!(layout_for(&i), ptt);
    }

    #[test]
    fn the_flow_pill_caption_row_adds_42_and_widens_to_464() {
        let plain = layout_for(&input(Style::FlowPill, Phase::Listening));
        let mut live = input(Style::FlowPill, Phase::Listening);
        live.has_live = true;
        let with_caption = layout_for(&live);
        assert!(!plain.caption && with_caption.caption);
        assert_eq!(with_caption.shape, plain.shape);
        assert_eq!(with_caption.window, sz(464.0 + 56.0, 32.0 + 42.0 + 52.0));
    }

    // Fitter.

    #[test]
    fn fitter_grows_at_once_to_the_union() {
        let mut f = Fitter::new(sz(100.0, 60.0));
        let fit = f.request(sz(200.0, 90.0));
        assert_eq!(fit.now, sz(200.0, 90.0));
        assert_eq!(fit.later, None);
        assert_eq!(f.current(), sz(200.0, 90.0));
    }

    #[test]
    fn fitter_keeps_the_old_size_when_the_target_is_smaller() {
        let mut f = Fitter::new(sz(200.0, 90.0));
        let fit = f.request(sz(100.0, 60.0));
        assert_eq!(fit.now, sz(200.0, 90.0));
        let later = fit.later.expect("a shrink is scheduled");
        assert_eq!(later.size, sz(100.0, 60.0));
        assert_eq!(f.current(), sz(200.0, 90.0));
    }

    #[test]
    fn fitter_takes_the_larger_side_of_each_dimension() {
        let mut f = Fitter::new(sz(100.0, 90.0));
        let fit = f.request(sz(200.0, 60.0));
        assert_eq!(fit.now, sz(200.0, 90.0));
        assert_eq!(fit.later.expect("shrink").size, sz(200.0, 60.0));
    }

    #[test]
    fn fitter_shrinks_when_the_latest_generation_comes_due() {
        let mut f = Fitter::new(sz(200.0, 90.0));
        let later = f.request(sz(100.0, 60.0)).later.expect("shrink");
        assert_eq!(f.shrink_due(later.generation), Some(sz(100.0, 60.0)));
        assert_eq!(f.current(), sz(100.0, 60.0));
    }

    #[test]
    fn fitter_ignores_a_stale_generation() {
        let mut f = Fitter::new(sz(200.0, 90.0));
        let first = f.request(sz(100.0, 60.0)).later.expect("shrink");
        let second = f.request(sz(150.0, 70.0)).later.expect("shrink");
        assert_ne!(first.generation, second.generation);
        assert_eq!(f.shrink_due(first.generation), None);
        assert_eq!(f.current(), sz(200.0, 90.0));
        assert_eq!(f.shrink_due(second.generation), Some(sz(150.0, 70.0)));
    }

    #[test]
    fn a_grow_cancels_a_pending_shrink() {
        let mut f = Fitter::new(sz(200.0, 90.0));
        let pending = f.request(sz(100.0, 60.0)).later.expect("shrink");
        let fit = f.request(sz(300.0, 100.0));
        assert_eq!(fit.now, sz(300.0, 100.0));
        assert_eq!(f.shrink_due(pending.generation), None);
        assert_eq!(f.current(), sz(300.0, 100.0));
    }

    #[test]
    fn a_request_for_the_current_size_changes_nothing() {
        let mut f = Fitter::new(sz(200.0, 90.0));
        let fit = f.request(sz(200.0, 90.0));
        assert_eq!((fit.now, fit.later), (sz(200.0, 90.0), None));
    }

    #[test]
    fn a_shrink_runs_once() {
        let mut f = Fitter::new(sz(200.0, 90.0));
        let later = f.request(sz(100.0, 60.0)).later.expect("shrink");
        assert!(f.shrink_due(later.generation).is_some());
        assert_eq!(f.shrink_due(later.generation), None);
    }

    // frame_for.

    const WORK_1080: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };

    #[test]
    fn frame_centres_on_the_anchor_with_the_bottom_edge_on_it() {
        assert_eq!(
            frame_for(pt(960, 1032), (96, 60), WORK_1080, 1.0),
            rect(912, 972, 96, 60)
        );
    }

    #[test]
    fn frame_is_clamped_to_the_left_and_right_edges_at_scale_1() {
        assert_eq!(frame_for(pt(10, 1032), (96, 60), WORK_1080, 1.0).x, 8);
        assert_eq!(
            frame_for(pt(1915, 1032), (96, 60), WORK_1080, 1.0).x,
            1920 - 96 - 8
        );
    }

    #[test]
    fn frame_is_clamped_to_the_top_and_bottom_edges_at_scale_1() {
        assert_eq!(frame_for(pt(960, 20), (96, 60), WORK_1080, 1.0).y, 8);
        assert_eq!(
            frame_for(pt(960, 1100), (96, 60), WORK_1080, 1.0).y,
            1040 - 60 - 8
        );
    }

    #[test]
    fn frame_margin_is_12_pixels_at_scale_1_5() {
        let work = rect(0, 0, 2560, 1400);
        assert_eq!(
            frame_for(pt(1280, 1388), (144, 90), work, 1.5),
            rect(1208, 1298, 144, 90)
        );
        assert_eq!(frame_for(pt(0, 1388), (144, 90), work, 1.5).x, 12);
        assert_eq!(
            frame_for(pt(2560, 1388), (144, 90), work, 1.5).x,
            2560 - 144 - 12
        );
        assert_eq!(frame_for(pt(1280, 5), (144, 90), work, 1.5).y, 12);
        assert_eq!(
            frame_for(pt(1280, 1500), (144, 90), work, 1.5).y,
            1400 - 90 - 12
        );
    }

    #[test]
    fn frame_on_a_monitor_left_of_the_primary() {
        let work = rect(-1280, 0, 1280, 984);
        assert_eq!(
            frame_for(pt(-640, 976), (96, 60), work, 1.0),
            rect(-688, 916, 96, 60)
        );
        assert_eq!(frame_for(pt(-1279, 976), (96, 60), work, 1.0).x, -1272);
        assert_eq!(frame_for(pt(-1, 976), (96, 60), work, 1.0).x, -104);
    }

    #[test]
    fn frame_on_a_monitor_above_the_primary_with_a_negative_y() {
        let work = rect(0, -1040, 1920, 1040);
        assert_eq!(frame_for(pt(960, -1100), (96, 60), work, 1.0).y, -1032);
        assert_eq!(
            frame_for(pt(960, 50), (96, 60), work, 1.0).y,
            -1040 + 1040 - 60 - 8
        );
    }

    #[test]
    fn frame_in_a_work_area_smaller_than_the_window_keeps_the_low_edge() {
        assert_eq!(
            frame_for(pt(130, 120), (96, 60), rect(100, 50, 60, 70), 1.0),
            rect(108, 58, 96, 60)
        );
    }

    // Anchors and monitors.

    fn two_monitors() -> [Monitor; 2] {
        [mon(0, 0, 1920, 1040, 1.0), mon(1920, 0, 2560, 1400, 1.5)]
    }

    #[test]
    fn monitor_for_finds_the_monitor_that_holds_the_anchor() {
        let m = two_monitors();
        assert_eq!(monitor_for(pt(960, 500), &m), Some(0));
        assert_eq!(monitor_for(pt(3000, 500), &m), Some(1));
    }

    #[test]
    fn monitor_for_allows_16_points_below_the_work_area() {
        let m = [mon(0, 0, 1920, 1040, 1.0)];
        assert_eq!(monitor_for(pt(960, 1050), &m), Some(0));
        assert_eq!(monitor_for(pt(960, 1060), &m), None);
    }

    #[test]
    fn monitor_for_allows_2_points_either_side() {
        let m = [mon(100, 0, 1920, 1040, 1.0)];
        assert_eq!(monitor_for(pt(99, 500), &m), Some(0));
        assert_eq!(monitor_for(pt(97, 500), &m), None);
        assert_eq!(monitor_for(pt(2021, 500), &m), Some(0));
        assert_eq!(monitor_for(pt(2023, 500), &m), None);
    }

    #[test]
    fn monitor_for_scales_the_slack() {
        let m = [mon(0, 0, 2560, 1400, 1.5)];
        assert_eq!(monitor_for(pt(1000, 1420), &m), Some(0));
        assert_eq!(monitor_for(pt(1000, 1430), &m), None);
    }

    #[test]
    fn monitor_for_no_monitors_is_none() {
        assert_eq!(monitor_for(pt(0, 0), &[]), None);
    }

    #[test]
    fn default_anchor_is_bottom_centre_8_points_up() {
        assert_eq!(default_anchor(&mon(0, 0, 1920, 1040, 1.0)), pt(960, 1032));
        assert_eq!(default_anchor(&mon(0, 0, 2560, 1400, 1.5)), pt(1280, 1388));
        assert_eq!(
            default_anchor(&mon(-1280, 0, 1280, 984, 1.0)),
            pt(-640, 976)
        );
    }

    #[test]
    fn a_saved_anchor_on_a_monitor_is_kept() {
        assert_eq!(
            resolve_anchor(Some(pt(3000, 900)), &two_monitors()),
            pt(3000, 900)
        );
    }

    #[test]
    fn a_saved_anchor_off_every_monitor_falls_back_to_the_primary_default() {
        assert_eq!(
            resolve_anchor(Some(pt(9000, 900)), &two_monitors()),
            pt(960, 1032)
        );
    }

    #[test]
    fn no_saved_anchor_gives_the_primary_default() {
        assert_eq!(resolve_anchor(None, &two_monitors()), pt(960, 1032));
    }

    // PointerGesture.

    #[test]
    fn a_quick_press_and_release_is_a_click() {
        let mut g = PointerGesture::new(1.0);
        assert_eq!(g.down(0.0, 100.0, 100.0), PointerOut::None);
        assert_eq!(g.up(0.1), PointerOut::Click);
    }

    #[test]
    fn a_wiggle_under_the_threshold_is_still_a_click() {
        let mut g = PointerGesture::new(1.0);
        g.down(0.0, 100.0, 100.0);
        assert_eq!(g.moved(103.0, 100.0), PointerOut::None);
        assert_eq!(g.up(0.1), PointerOut::Click);
    }

    #[test]
    fn holding_for_0_35_seconds_begins_a_hold_once() {
        let mut g = PointerGesture::new(1.0);
        g.down(0.0, 100.0, 100.0);
        assert_eq!(g.tick(0.34), PointerOut::None);
        assert_eq!(g.tick(0.35), PointerOut::HoldBegan);
        assert_eq!(g.tick(0.5), PointerOut::None);
    }

    #[test]
    fn releasing_after_a_hold_ends_it_and_is_not_a_click() {
        let mut g = PointerGesture::new(1.0);
        g.down(0.0, 100.0, 100.0);
        g.tick(0.4);
        assert_eq!(g.up(1.0), PointerOut::HoldEnded);
    }

    #[test]
    fn moving_during_a_hold_does_not_drag() {
        let mut g = PointerGesture::new(1.0);
        g.down(0.0, 100.0, 100.0);
        g.tick(0.4);
        assert_eq!(g.moved(200.0, 200.0), PointerOut::None);
        assert_eq!(g.up(1.0), PointerOut::HoldEnded);
    }

    #[test]
    fn moving_4_points_starts_a_drag_with_the_full_distance() {
        let mut g = PointerGesture::new(1.0);
        g.down(0.0, 100.0, 100.0);
        assert_eq!(g.moved(103.0, 100.0), PointerOut::None);
        assert_eq!(
            g.moved(104.0, 100.0),
            PointerOut::Dragged { dx: 4.0, dy: 0.0 }
        );
        assert_eq!(
            g.moved(110.0, 97.0),
            PointerOut::Dragged { dx: 6.0, dy: -3.0 }
        );
    }

    #[test]
    fn a_drag_cancels_the_hold_and_ends_with_drag_ended() {
        let mut g = PointerGesture::new(1.0);
        g.down(0.0, 100.0, 100.0);
        g.moved(110.0, 100.0);
        assert_eq!(g.tick(0.5), PointerOut::None);
        assert_eq!(g.up(0.6), PointerOut::DragEnded);
    }

    #[test]
    fn the_drag_threshold_scales_with_the_display() {
        let mut g = PointerGesture::new(1.5);
        g.down(0.0, 100.0, 100.0);
        assert_eq!(g.moved(105.0, 100.0), PointerOut::None);
        assert_eq!(
            g.moved(106.0, 100.0),
            PointerOut::Dragged { dx: 6.0, dy: 0.0 }
        );
    }

    #[test]
    fn a_new_press_starts_clean_after_a_drag() {
        let mut g = PointerGesture::new(1.0);
        g.down(0.0, 100.0, 100.0);
        g.moved(120.0, 100.0);
        g.up(0.2);
        g.down(5.0, 100.0, 100.0);
        assert_eq!(g.up(5.1), PointerOut::Click);
    }

    #[test]
    fn events_without_a_press_do_nothing() {
        let mut g = PointerGesture::new(1.0);
        assert_eq!(g.up(0.1), PointerOut::None);
        assert_eq!(g.tick(5.0), PointerOut::None);
        assert_eq!(g.moved(50.0, 50.0), PointerOut::None);
    }
}
