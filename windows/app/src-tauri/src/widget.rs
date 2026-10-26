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
}
