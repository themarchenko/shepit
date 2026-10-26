//! Pure filter from raw low-level keyboard events to hotkey and Esc events. The hook callback
//! only copies fields; everything that decides lives here so it runs in tests.

pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
pub const WM_SYSKEYDOWN: u32 = 0x0104;
pub const WM_SYSKEYUP: u32 = 0x0105;

pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_CONTROL: u32 = 0x11;
pub const VK_RCONTROL: u32 = 0xA3;

pub const LLKHF_EXTENDED: u32 = 0x01;
pub const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;
pub const LLKHF_INJECTED: u32 = 0x10;

/// The fields of KBDLLHOOKSTRUCT the filter needs, plus the window message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKey {
    pub message: u32,
    pub vk: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    HotkeyDown,
    HotkeyUp,
    EscPressed,
}

/// True only for the configured hotkey (and the generic VK_CONTROL the driver may report for it
/// when the extended flag is set) and Esc. The hook calls this before it sends anything, so no
/// other key ever leaves the hook proc.
pub fn is_accepted(vk: u32, flags: u32, hotkey_vk: u32) -> bool {
    vk == VK_ESCAPE
        || vk == hotkey_vk
        || (hotkey_vk == VK_RCONTROL && vk == VK_CONTROL && flags & LLKHF_EXTENDED != 0)
}

#[derive(Debug, Clone, Default)]
pub struct KeyFilter {
    hotkey_down: bool,
    esc_down: bool,
}

impl KeyFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an event only for a real Right Ctrl edge or a fresh Esc press.
    pub fn filter(&mut self, raw: RawKey) -> Option<KeyEvent> {
        // Injected input (our own paste, other tools) must never drive the hotkey.
        if raw.flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0 {
            return None;
        }
        let down = matches!(raw.message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let up = matches!(raw.message, WM_KEYUP | WM_SYSKEYUP);
        if !down && !up {
            return None;
        }
        // Some drivers report the generic VK_CONTROL; the extended flag marks the right key.
        let is_hotkey =
            raw.vk == VK_RCONTROL || (raw.vk == VK_CONTROL && raw.flags & LLKHF_EXTENDED != 0);
        if is_hotkey {
            if down && !self.hotkey_down {
                self.hotkey_down = true;
                return Some(KeyEvent::HotkeyDown);
            }
            if up && self.hotkey_down {
                self.hotkey_down = false;
                return Some(KeyEvent::HotkeyUp);
            }
            return None;
        }
        if raw.vk == VK_ESCAPE {
            if down && !self.esc_down {
                self.esc_down = true;
                return Some(KeyEvent::EscPressed);
            }
            if up {
                self.esc_down = false;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(message: u32, vk: u32, flags: u32) -> RawKey {
        RawKey { message, vk, flags }
    }
    fn rctrl_down() -> RawKey {
        raw(WM_KEYDOWN, VK_RCONTROL, LLKHF_EXTENDED)
    }
    fn rctrl_up() -> RawKey {
        raw(WM_KEYUP, VK_RCONTROL, LLKHF_EXTENDED)
    }

    #[test]
    fn only_the_hotkey_and_esc_are_accepted() {
        // C, A, Left Ctrl, Space, Enter
        for vk in [0x43, 0x41, 0xA2, 0x20, 0x0D] {
            assert!(!is_accepted(vk, 0, VK_RCONTROL), "{vk:#x}");
            assert!(!is_accepted(vk, LLKHF_EXTENDED, VK_RCONTROL), "{vk:#x}");
        }
        assert!(is_accepted(VK_RCONTROL, LLKHF_EXTENDED, VK_RCONTROL));
        assert!(is_accepted(VK_CONTROL, LLKHF_EXTENDED, VK_RCONTROL));
        assert!(is_accepted(VK_ESCAPE, 0, VK_RCONTROL));
    }

    #[test]
    fn generic_ctrl_without_extended_flag_is_not_accepted() {
        assert!(!is_accepted(VK_CONTROL, 0, VK_RCONTROL));
    }

    #[test]
    fn right_ctrl_vk_a3_matches() {
        let mut f = KeyFilter::new();
        assert_eq!(f.filter(rctrl_down()), Some(KeyEvent::HotkeyDown));
        assert_eq!(f.filter(rctrl_up()), Some(KeyEvent::HotkeyUp));
    }

    #[test]
    fn generic_ctrl_with_extended_flag_matches() {
        let mut f = KeyFilter::new();
        let down = raw(WM_KEYDOWN, VK_CONTROL, LLKHF_EXTENDED);
        assert_eq!(f.filter(down), Some(KeyEvent::HotkeyDown));
    }

    #[test]
    fn generic_ctrl_without_extended_flag_is_left_ctrl() {
        let mut f = KeyFilter::new();
        assert_eq!(f.filter(raw(WM_KEYDOWN, VK_CONTROL, 0)), None);
    }

    #[test]
    fn left_ctrl_vk_a2_does_not_match() {
        let mut f = KeyFilter::new();
        assert_eq!(f.filter(raw(WM_KEYDOWN, 0xA2, 0)), None);
        assert_eq!(f.filter(raw(WM_KEYUP, 0xA2, 0)), None);
    }

    #[test]
    fn sys_messages_count_as_down_and_up() {
        let mut f = KeyFilter::new();
        let down = raw(WM_SYSKEYDOWN, VK_RCONTROL, LLKHF_EXTENDED);
        let up = raw(WM_SYSKEYUP, VK_RCONTROL, LLKHF_EXTENDED);
        assert_eq!(f.filter(down), Some(KeyEvent::HotkeyDown));
        assert_eq!(f.filter(up), Some(KeyEvent::HotkeyUp));
    }

    #[test]
    fn repeated_down_gives_one_event() {
        let mut f = KeyFilter::new();
        assert_eq!(f.filter(rctrl_down()), Some(KeyEvent::HotkeyDown));
        assert_eq!(f.filter(rctrl_down()), None);
        assert_eq!(f.filter(rctrl_down()), None);
    }

    #[test]
    fn up_without_down_gives_nothing() {
        let mut f = KeyFilter::new();
        assert_eq!(f.filter(rctrl_up()), None);
    }

    #[test]
    fn injected_events_are_ignored() {
        let mut f = KeyFilter::new();
        let down = raw(WM_KEYDOWN, VK_RCONTROL, LLKHF_EXTENDED | LLKHF_INJECTED);
        let esc = raw(WM_KEYDOWN, VK_ESCAPE, LLKHF_INJECTED);
        assert_eq!(f.filter(down), None);
        assert_eq!(f.filter(esc), None);
    }

    #[test]
    fn lower_integrity_injected_events_are_ignored() {
        let mut f = KeyFilter::new();
        let down = raw(
            WM_KEYDOWN,
            VK_RCONTROL,
            LLKHF_EXTENDED | LLKHF_LOWER_IL_INJECTED,
        );
        let esc = raw(WM_KEYDOWN, VK_ESCAPE, LLKHF_LOWER_IL_INJECTED);
        assert_eq!(f.filter(down), None);
        assert_eq!(f.filter(esc), None);
    }

    #[test]
    fn esc_fires_once_per_press() {
        let mut f = KeyFilter::new();
        let down = raw(WM_KEYDOWN, VK_ESCAPE, 0);
        let up = raw(WM_KEYUP, VK_ESCAPE, 0);
        assert_eq!(f.filter(down), Some(KeyEvent::EscPressed));
        assert_eq!(f.filter(down), None);
        assert_eq!(f.filter(up), None);
        assert_eq!(f.filter(down), Some(KeyEvent::EscPressed));
    }

    #[test]
    fn other_keys_during_a_hold_are_ignored_and_the_release_still_comes() {
        let mut f = KeyFilter::new();
        assert_eq!(f.filter(rctrl_down()), Some(KeyEvent::HotkeyDown));
        assert_eq!(f.filter(raw(WM_KEYDOWN, 0x43, 0)), None);
        assert_eq!(f.filter(raw(WM_KEYUP, 0x43, 0)), None);
        assert_eq!(f.filter(rctrl_up()), Some(KeyEvent::HotkeyUp));
    }
}
