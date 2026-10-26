/// Top-left position for the widget: centred horizontally, bottom edge `gap` above the
/// work area bottom. `work_area` is (x, y, width, height); y grows downward.
pub fn default_position(
    work_area: (i32, i32, i32, i32),
    window: (i32, i32),
    gap: i32,
) -> (i32, i32) {
    let (wx, wy, w, h) = work_area;
    (wx + w / 2 - window.0 / 2, wy + h - gap - window.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_on_primary_1080p() {
        assert_eq!(
            default_position((0, 0, 1920, 1040), (96, 60), 4),
            (912, 976)
        );
    }

    #[test]
    fn larger_window_and_gap() {
        assert_eq!(
            default_position((0, 0, 1920, 1040), (144, 90), 6),
            (888, 944)
        );
    }

    #[test]
    fn work_area_with_negative_origin() {
        assert_eq!(
            default_position((-1280, 0, 1280, 984), (96, 60), 4),
            (-688, 920)
        );
    }
}
