//! The update card window: a small glass window at the bottom right of the work area. STUBS: the
//! tests define the behaviour; the bodies are `todo!()` until the implementer fills them in.
#![allow(dead_code, unused_variables)]

use shepit_core::widget::Rect;

/// 380 logical pixels of card plus 20 on each side for the margin (UpdatePanel.swift:22, 130).
pub const CARD_WIDTH: f64 = 420.0;
pub const MIN_HEIGHT: f64 = 60.0;
pub const MAX_HEIGHT: f64 = 480.0;

/// The card's frame in physical pixels, its bottom-right corner on (`right`, `bottom`). `height`
/// is logical and is clamped to `MIN_HEIGHT..=MAX_HEIGHT`; NaN counts as the minimum.
pub fn card_frame(right: i32, bottom: i32, scale: f64, height: f64) -> Rect {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(right: i32, bottom: i32, scale: f64, height: f64) -> (i32, i32, i32, i32) {
        let r = card_frame(right, bottom, scale, height);
        (r.x, r.y, r.width, r.height)
    }

    #[test]
    fn the_card_keeps_its_bottom_right_corner_at_100_percent() {
        assert_eq!(frame(1920, 1040, 1.0, 200.0), (1500, 840, 420, 200));
        // A taller card grows upward and the corner stays.
        assert_eq!(frame(1920, 1040, 1.0, 320.0), (1500, 720, 420, 320));
    }

    #[test]
    fn the_card_scales_with_the_display_at_150_percent() {
        assert_eq!(frame(2880, 1560, 1.5, 200.0), (2250, 1260, 630, 300));
    }

    #[test]
    fn sizes_round_to_whole_pixels() {
        // 420 * 1.25 = 525 and 201 * 1.25 = 251.25.
        assert_eq!(frame(2400, 1300, 1.25, 201.0), (1875, 1049, 525, 251));
    }

    #[test]
    fn the_height_is_clamped() {
        assert_eq!(frame(1920, 1040, 1.0, 10.0).3, 60);
        assert_eq!(frame(1920, 1040, 1.0, 5000.0).3, 480);
        assert_eq!(frame(1920, 1040, 1.5, 5000.0).3, 720);
        assert_eq!(frame(1920, 1040, 1.0, -3.0).3, 60);
    }

    #[test]
    fn a_nan_height_counts_as_the_minimum() {
        assert_eq!(frame(1920, 1040, 1.0, f64::NAN), (1500, 980, 420, 60));
    }
}
