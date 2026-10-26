//! Hotkey gestures from raw key down/up times. Mirrors `HotkeyGesture` in the Swift core.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Output {
    None,
    PressBegan,
    PressEnded { duration: f64 },
    ShortTap,
    DoubleTap,
}

#[derive(Debug, Clone)]
pub struct HotkeyGesture {
    pub short_tap_max: f64,
    pub double_tap_window: f64,
    pressed_at: Option<f64>,
    last_short_tap_at: Option<f64>,
}

impl HotkeyGesture {
    /// Defaults match Swift: 0.25 s short tap, 0.45 s double-tap window.
    pub fn new() -> Self {
        Self {
            // HotkeyGesture.swift:15
            short_tap_max: 0.25,
            // HotkeyGesture.swift:16
            double_tap_window: 0.45,
            pressed_at: None,
            last_short_tap_at: None,
        }
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed_at.is_some()
    }

    pub fn key_down(&mut self, at_secs: f64) -> Output {
        // A repeated down without an up is ignored (auto-repeat).
        if self.pressed_at.is_some() {
            return Output::None;
        }
        self.pressed_at = Some(at_secs);
        Output::PressBegan
    }

    pub fn key_up(&mut self, at_secs: f64) -> Output {
        let Some(start) = self.pressed_at.take() else {
            return Output::None;
        };
        let duration = at_secs - start;
        if duration <= self.short_tap_max {
            if let Some(last) = self.last_short_tap_at {
                if at_secs - last <= self.double_tap_window {
                    self.last_short_tap_at = None;
                    return Output::DoubleTap;
                }
            }
            self.last_short_tap_at = Some(at_secs);
            return Output::ShortTap;
        }
        self.last_short_tap_at = None;
        Output::PressEnded { duration }
    }
}

impl Default for HotkeyGesture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_repeat_down_is_ignored_and_stray_up_is_none() {
        let mut g = HotkeyGesture::new();
        assert_eq!(g.key_up(0.0), Output::None);
        assert_eq!(g.key_down(1.0), Output::PressBegan);
        assert!(g.is_pressed());
        assert_eq!(g.key_down(1.1), Output::None);
    }

    #[test]
    fn long_press_clears_pending_short_tap() {
        let mut g = HotkeyGesture::new();
        g.key_down(0.0);
        assert_eq!(g.key_up(0.1), Output::ShortTap);
        g.key_down(0.2);
        assert!(matches!(g.key_up(1.0), Output::PressEnded { .. }));
        g.key_down(1.1);
        assert_eq!(g.key_up(1.2), Output::ShortTap);
    }

    #[test]
    fn double_tap_window_boundary() {
        // Exact binary fractions avoid float noise. Taps ending 0.25 s apart pair up; 0.5 s apart
        // (beyond the 0.45 s window) do not.
        let mut g = HotkeyGesture::new();
        g.key_down(0.0);
        g.key_up(0.125);
        g.key_down(0.25);
        assert_eq!(g.key_up(0.375), Output::DoubleTap);
        let mut g = HotkeyGesture::new();
        g.key_down(0.0);
        g.key_up(0.125);
        g.key_down(0.5);
        assert_eq!(g.key_up(0.625), Output::ShortTap);
    }
}
