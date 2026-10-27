//! Pure filter from raw low-level keyboard events to hotkey and Esc events. The hook callback
//! only copies fields; everything that decides lives here so it runs in tests.

pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
pub const WM_SYSKEYDOWN: u32 = 0x0104;
pub const WM_SYSKEYUP: u32 = 0x0105;

pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_CONTROL: u32 = 0x11;
pub const VK_RCONTROL: u32 = 0xA3;
pub const VK_MENU: u32 = 0x12;
pub const VK_RMENU: u32 = 0xA5;
pub const VK_F13: u32 = 0x7C;
pub const VK_F24: u32 = 0x87;

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

/// The keys the user can pick for dictation (docs: plan-windows-settings.md decision 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hotkey {
    #[default]
    RightCtrl,
    RightAlt,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
}

impl Hotkey {
    pub const ALL: [Hotkey; 14] = [
        Hotkey::RightCtrl,
        Hotkey::RightAlt,
        Hotkey::F13,
        Hotkey::F14,
        Hotkey::F15,
        Hotkey::F16,
        Hotkey::F17,
        Hotkey::F18,
        Hotkey::F19,
        Hotkey::F20,
        Hotkey::F21,
        Hotkey::F22,
        Hotkey::F23,
        Hotkey::F24,
    ];

    /// The persisted id: "rightCtrl", "rightAlt", "f13" to "f24".
    pub fn id(self) -> &'static str {
        const FN_IDS: [&str; 12] = [
            "f13", "f14", "f15", "f16", "f17", "f18", "f19", "f20", "f21", "f22", "f23", "f24",
        ];
        match self {
            Hotkey::RightCtrl => "rightCtrl",
            Hotkey::RightAlt => "rightAlt",
            other => FN_IDS[other.fn_index()],
        }
    }

    pub fn from_id(id: &str) -> Option<Hotkey> {
        Hotkey::ALL.into_iter().find(|h| h.id() == id)
    }

    /// The name the UI and the widget show: "Right Ctrl", "Right Alt", "F13".
    pub fn label(self) -> &'static str {
        const FN_LABELS: [&str; 12] = [
            "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
        ];
        match self {
            Hotkey::RightCtrl => "Right Ctrl",
            Hotkey::RightAlt => "Right Alt",
            other => FN_LABELS[other.fn_index()],
        }
    }

    /// Position among F13..F24. Only called for those variants.
    fn fn_index(self) -> usize {
        Hotkey::ALL
            .iter()
            .position(|h| *h == self)
            .map_or(0, |i| i.saturating_sub(2))
    }

    /// True when the raw event is this key. Right Ctrl and Right Alt also arrive as the generic
    /// VK_CONTROL / VK_MENU with the extended flag.
    pub fn matches(self, vk: u32, flags: u32) -> bool {
        let extended = flags & LLKHF_EXTENDED != 0;
        match self {
            Hotkey::RightCtrl => vk == VK_RCONTROL || (vk == VK_CONTROL && extended),
            Hotkey::RightAlt => vk == VK_RMENU || (vk == VK_MENU && extended),
            other => vk == VK_F13 + other.fn_index() as u32,
        }
    }

    /// The hotkey a raw event stands for, or None for every key the picker refuses.
    pub fn candidate(vk: u32, flags: u32) -> Option<Hotkey> {
        Hotkey::ALL.into_iter().find(|h| h.matches(vk, flags))
    }
}

/// True only for the configured hotkey and Esc. While `picking`, every candidate key also passes
/// so the picker can see it. The hook calls this before it sends anything, so no other key ever
/// leaves the hook proc.
pub fn is_accepted(vk: u32, flags: u32, hotkey: Hotkey, picking: bool) -> bool {
    vk == VK_ESCAPE
        || hotkey.matches(vk, flags)
        || (picking && Hotkey::candidate(vk, flags).is_some())
}

#[derive(Debug, Clone, Default)]
pub struct KeyFilter {
    hotkey: Hotkey,
    hotkey_down: bool,
    esc_down: bool,
}

impl KeyFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_hotkey(hotkey: Hotkey) -> Self {
        Self {
            hotkey,
            ..Self::default()
        }
    }

    /// Returns an event only for a real edge of the chosen key or a fresh Esc press.
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
        if self.hotkey.matches(raw.vk, raw.flags) {
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

/// How a key pick ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickResult {
    Picked(Hotkey),
    Cancelled,
    TimedOut,
}

/// A pick gives up after this many seconds.
pub const PICK_TIMEOUT_SECS: f64 = 15.0;

/// Waits for the next candidate key-down. Pure: the app feeds it raw events and the clock.
#[derive(Debug, Clone)]
pub struct HotkeyPicker {
    started_at: f64,
    done: bool,
}

impl HotkeyPicker {
    pub fn new(started_at: f64) -> Self {
        Self {
            started_at,
            done: false,
        }
    }

    /// The first candidate key-down picks it, Esc cancels. Key-ups, injected events and every
    /// other key give None, and so does everything after a result.
    pub fn on_key(&mut self, raw: RawKey) -> Option<PickResult> {
        if self.done || raw.flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0 {
            return None;
        }
        if !matches!(raw.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
            return None;
        }
        let result = if raw.vk == VK_ESCAPE {
            PickResult::Cancelled
        } else {
            PickResult::Picked(Hotkey::candidate(raw.vk, raw.flags)?)
        };
        self.done = true;
        Some(result)
    }

    /// TimedOut once `PICK_TIMEOUT_SECS` have passed since `started_at`.
    pub fn poll(&mut self, now: f64) -> Option<PickResult> {
        if self.done || now - self.started_at < PICK_TIMEOUT_SECS {
            return None;
        }
        self.done = true;
        Some(PickResult::TimedOut)
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
            assert!(!is_accepted(vk, 0, Hotkey::RightCtrl, false), "{vk:#x}");
            assert!(
                !is_accepted(vk, LLKHF_EXTENDED, Hotkey::RightCtrl, false),
                "{vk:#x}"
            );
        }
        assert!(is_accepted(
            VK_RCONTROL,
            LLKHF_EXTENDED,
            Hotkey::RightCtrl,
            false
        ));
        assert!(is_accepted(
            VK_CONTROL,
            LLKHF_EXTENDED,
            Hotkey::RightCtrl,
            false
        ));
        assert!(is_accepted(VK_ESCAPE, 0, Hotkey::RightCtrl, false));
    }

    #[test]
    fn generic_ctrl_without_extended_flag_is_not_accepted() {
        assert!(!is_accepted(VK_CONTROL, 0, Hotkey::RightCtrl, false));
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

    // Hotkey ids, labels and matching.

    const EXPECTED_IDS: [&str; 14] = [
        "rightCtrl",
        "rightAlt",
        "f13",
        "f14",
        "f15",
        "f16",
        "f17",
        "f18",
        "f19",
        "f20",
        "f21",
        "f22",
        "f23",
        "f24",
    ];

    #[test]
    fn the_default_hotkey_is_right_ctrl() {
        assert_eq!(Hotkey::default(), Hotkey::RightCtrl);
    }

    #[test]
    fn ids_are_the_persisted_names_in_order() {
        let ids: Vec<&str> = Hotkey::ALL.iter().map(|h| h.id()).collect();
        assert_eq!(ids, EXPECTED_IDS);
    }

    #[test]
    fn ids_round_trip_for_all_14_keys() {
        for h in Hotkey::ALL {
            assert_eq!(Hotkey::from_id(h.id()), Some(h), "{h:?}");
        }
    }

    #[test]
    fn an_unknown_id_gives_none() {
        for bad in [
            "",
            "RightCtrl",
            "rightctrl",
            "leftCtrl",
            "rightShift",
            "f12",
            "f25",
            "f1",
            "F13",
            " f13",
            "capsLock",
        ] {
            assert_eq!(Hotkey::from_id(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn labels_are_what_the_user_sees() {
        assert_eq!(Hotkey::RightCtrl.label(), "Right Ctrl");
        assert_eq!(Hotkey::RightAlt.label(), "Right Alt");
        for (i, h) in Hotkey::ALL[2..].iter().enumerate() {
            assert_eq!(h.label(), format!("F{}", 13 + i), "{h:?}");
        }
    }

    #[test]
    fn right_ctrl_matches_a3_and_the_generic_ctrl_with_the_extended_flag() {
        let h = Hotkey::RightCtrl;
        assert!(h.matches(VK_RCONTROL, 0));
        assert!(h.matches(VK_RCONTROL, LLKHF_EXTENDED));
        assert!(h.matches(VK_CONTROL, LLKHF_EXTENDED));
        assert!(!h.matches(VK_CONTROL, 0));
        assert!(!h.matches(0xA2, 0));
        assert!(!h.matches(0xA2, LLKHF_EXTENDED));
    }

    #[test]
    fn right_alt_matches_a5_and_the_generic_alt_with_the_extended_flag() {
        let h = Hotkey::RightAlt;
        assert!(h.matches(VK_RMENU, 0));
        assert!(h.matches(VK_RMENU, LLKHF_EXTENDED));
        assert!(h.matches(VK_MENU, LLKHF_EXTENDED));
        assert!(!h.matches(VK_MENU, 0));
        // Left Alt
        assert!(!h.matches(0xA4, 0));
        assert!(!h.matches(0xA4, LLKHF_EXTENDED));
        assert!(!h.matches(VK_RCONTROL, LLKHF_EXTENDED));
    }

    #[test]
    fn the_function_keys_match_0x7c_through_0x87_and_only_themselves() {
        for (i, h) in Hotkey::ALL[2..].iter().enumerate() {
            let vk = VK_F13 + i as u32;
            assert!(h.matches(vk, 0), "{h:?}");
            assert!(h.matches(vk, LLKHF_EXTENDED), "{h:?}");
            assert!(!h.matches(vk + 1, 0), "{h:?} must not match the next key");
            assert!(
                !h.matches(vk - 1, 0),
                "{h:?} must not match the previous key"
            );
        }
        assert_eq!(VK_F24, VK_F13 + 11);
    }

    #[test]
    fn right_ctrl_and_right_alt_do_not_match_each_other() {
        assert!(!Hotkey::RightCtrl.matches(VK_RMENU, LLKHF_EXTENDED));
        assert!(!Hotkey::RightAlt.matches(VK_RCONTROL, LLKHF_EXTENDED));
    }

    #[test]
    fn candidate_names_the_key_for_each_accepted_event() {
        let cases = [
            (VK_RCONTROL, LLKHF_EXTENDED, Hotkey::RightCtrl),
            (VK_CONTROL, LLKHF_EXTENDED, Hotkey::RightCtrl),
            (VK_RMENU, LLKHF_EXTENDED, Hotkey::RightAlt),
            (VK_MENU, LLKHF_EXTENDED, Hotkey::RightAlt),
            (0x7C, 0, Hotkey::F13),
            (0x7F, 0, Hotkey::F16),
            (0x87, 0, Hotkey::F24),
        ];
        for (vk, flags, want) in cases {
            assert_eq!(Hotkey::candidate(vk, flags), Some(want), "{vk:#x}");
        }
    }

    #[test]
    fn candidate_refuses_the_unsafe_keys() {
        // Left Ctrl, generic Ctrl and Alt without the extended flag, Left Alt, Shifts, Win,
        // Caps Lock, Apps, Scroll Lock, Pause, F1 to F12, Space, a letter, Tab, Enter.
        for vk in [
            0xA2, 0x11, 0x12, 0xA4, 0x10, 0xA0, 0xA1, 0x5B, 0x5C, 0x14, 0x5D, 0x91, 0x13, 0x70,
            0x71, 0x7B, 0x20, 0x41, 0x09, 0x0D, 0x88,
        ] {
            assert_eq!(Hotkey::candidate(vk, 0), None, "{vk:#x}");
        }
        assert_eq!(Hotkey::candidate(0xA2, LLKHF_EXTENDED), None);
        assert_eq!(Hotkey::candidate(0x10, LLKHF_EXTENDED), None);
    }

    #[test]
    fn candidate_agrees_with_matches_for_every_vk() {
        for vk in 0..=255u32 {
            for flags in [0, LLKHF_EXTENDED] {
                let matching: Vec<Hotkey> = Hotkey::ALL
                    .into_iter()
                    .filter(|h| h.matches(vk, flags))
                    .collect();
                assert!(matching.len() <= 1, "{vk:#x}/{flags}: {matching:?}");
                assert_eq!(
                    Hotkey::candidate(vk, flags),
                    matching.first().copied(),
                    "{vk:#x}/{flags}"
                );
            }
        }
    }

    // The picking filter: the riskiest control of this task.

    #[test]
    fn picking_lets_out_only_candidates_and_esc() {
        for hotkey in Hotkey::ALL {
            for vk in 0..=255u32 {
                for flags in [0, LLKHF_EXTENDED] {
                    let want = vk == VK_ESCAPE || Hotkey::candidate(vk, flags).is_some();
                    assert_eq!(
                        is_accepted(vk, flags, hotkey, true),
                        want,
                        "{vk:#x}/{flags} with {hotkey:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn picking_lets_out_no_letter_digit_or_space() {
        for vk in (0x30..=0x39).chain(0x41..=0x5A).chain([0x20, 0x08, 0x0D]) {
            assert!(!is_accepted(vk, 0, Hotkey::RightCtrl, true), "{vk:#x}");
        }
    }

    #[test]
    fn without_picking_only_the_chosen_key_and_esc_pass() {
        for hotkey in Hotkey::ALL {
            for vk in 0..=255u32 {
                for flags in [0, LLKHF_EXTENDED] {
                    let want = vk == VK_ESCAPE || hotkey.matches(vk, flags);
                    assert_eq!(
                        is_accepted(vk, flags, hotkey, false),
                        want,
                        "{vk:#x}/{flags} with {hotkey:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_accepted_key_follows_the_chosen_hotkey() {
        assert!(is_accepted(
            VK_RMENU,
            LLKHF_EXTENDED,
            Hotkey::RightAlt,
            false
        ));
        assert!(!is_accepted(
            VK_RCONTROL,
            LLKHF_EXTENDED,
            Hotkey::RightAlt,
            false
        ));
        assert!(is_accepted(0x81, 0, Hotkey::F18, false));
        assert!(!is_accepted(0x7C, 0, Hotkey::F18, false));
    }

    // The filter for a chosen key.

    #[test]
    fn a_filter_for_right_alt_reports_a5_and_ignores_right_ctrl() {
        let mut f = KeyFilter::for_hotkey(Hotkey::RightAlt);
        assert_eq!(f.filter(rctrl_down()), None);
        assert_eq!(f.filter(rctrl_up()), None);
        let down = raw(WM_SYSKEYDOWN, VK_RMENU, LLKHF_EXTENDED);
        let up = raw(WM_SYSKEYUP, VK_RMENU, LLKHF_EXTENDED);
        assert_eq!(f.filter(down), Some(KeyEvent::HotkeyDown));
        assert_eq!(f.filter(down), None);
        assert_eq!(f.filter(up), Some(KeyEvent::HotkeyUp));
    }

    #[test]
    fn a_filter_for_right_alt_takes_the_generic_alt_with_the_extended_flag() {
        let mut f = KeyFilter::for_hotkey(Hotkey::RightAlt);
        assert_eq!(f.filter(raw(WM_KEYDOWN, VK_MENU, 0)), None);
        assert_eq!(
            f.filter(raw(WM_KEYDOWN, VK_MENU, LLKHF_EXTENDED)),
            Some(KeyEvent::HotkeyDown)
        );
    }

    #[test]
    fn a_filter_for_a_function_key_reports_it_and_still_gives_esc() {
        let mut f = KeyFilter::for_hotkey(Hotkey::F13);
        assert_eq!(
            f.filter(raw(WM_KEYDOWN, 0x7C, 0)),
            Some(KeyEvent::HotkeyDown)
        );
        assert_eq!(f.filter(raw(WM_KEYUP, 0x7C, 0)), Some(KeyEvent::HotkeyUp));
        assert_eq!(f.filter(raw(WM_KEYDOWN, 0x7D, 0)), None);
        assert_eq!(
            f.filter(raw(WM_KEYDOWN, VK_ESCAPE, 0)),
            Some(KeyEvent::EscPressed)
        );
    }

    #[test]
    fn a_filter_for_each_hotkey_drops_injected_events() {
        for h in Hotkey::ALL {
            let (vk, flags) = match h {
                Hotkey::RightCtrl => (VK_RCONTROL, LLKHF_EXTENDED),
                Hotkey::RightAlt => (VK_RMENU, LLKHF_EXTENDED),
                other => (VK_F13 + (other as u32 - Hotkey::F13 as u32), 0),
            };
            let mut f = KeyFilter::for_hotkey(h);
            assert_eq!(
                f.filter(raw(WM_KEYDOWN, vk, flags | LLKHF_INJECTED)),
                None,
                "{h:?}"
            );
            assert_eq!(
                f.filter(raw(WM_KEYDOWN, vk, flags)),
                Some(KeyEvent::HotkeyDown),
                "{h:?}"
            );
        }
    }

    // The picker.

    fn key_down(vk: u32, flags: u32) -> RawKey {
        raw(WM_KEYDOWN, vk, flags)
    }

    #[test]
    fn the_picker_picks_the_first_candidate_key_down() {
        let mut p = HotkeyPicker::new(0.0);
        assert_eq!(
            p.on_key(rctrl_down()),
            Some(PickResult::Picked(Hotkey::RightCtrl))
        );
        let mut p = HotkeyPicker::new(0.0);
        assert_eq!(
            p.on_key(key_down(0x7E, 0)),
            Some(PickResult::Picked(Hotkey::F15))
        );
    }

    #[test]
    fn right_alt_arrives_as_a_sys_key_down_and_is_picked() {
        let mut p = HotkeyPicker::new(0.0);
        let down = raw(WM_SYSKEYDOWN, VK_RMENU, LLKHF_EXTENDED);
        assert_eq!(p.on_key(down), Some(PickResult::Picked(Hotkey::RightAlt)));
    }

    #[test]
    fn the_picker_ignores_key_ups() {
        let mut p = HotkeyPicker::new(0.0);
        assert_eq!(p.on_key(rctrl_up()), None);
        assert_eq!(p.on_key(raw(WM_SYSKEYUP, VK_RMENU, LLKHF_EXTENDED)), None);
        assert_eq!(p.on_key(raw(WM_KEYUP, 0x7C, 0)), None);
        // Still waiting: the next key-down is the pick.
        assert_eq!(
            p.on_key(key_down(0x7C, 0)),
            Some(PickResult::Picked(Hotkey::F13))
        );
    }

    #[test]
    fn the_picker_ignores_injected_keys() {
        let mut p = HotkeyPicker::new(0.0);
        let injected = key_down(VK_RCONTROL, LLKHF_EXTENDED | LLKHF_INJECTED);
        let lower = key_down(0x7C, LLKHF_LOWER_IL_INJECTED);
        let esc = key_down(VK_ESCAPE, LLKHF_INJECTED);
        assert_eq!(p.on_key(injected), None);
        assert_eq!(p.on_key(lower), None);
        assert_eq!(p.on_key(esc), None);
        assert_eq!(
            p.on_key(rctrl_down()),
            Some(PickResult::Picked(Hotkey::RightCtrl))
        );
    }

    #[test]
    fn the_picker_ignores_keys_that_are_not_candidates() {
        let mut p = HotkeyPicker::new(0.0);
        // A, Left Ctrl, Right Shift, Win, Caps Lock, F12, generic Ctrl without the extended flag.
        for vk in [0x41, 0xA2, 0xA1, 0x5B, 0x14, 0x7B, VK_CONTROL] {
            assert_eq!(p.on_key(key_down(vk, 0)), None, "{vk:#x}");
        }
        assert_eq!(p.poll(1.0), None);
    }

    #[test]
    fn esc_cancels_the_pick() {
        let mut p = HotkeyPicker::new(0.0);
        assert_eq!(
            p.on_key(key_down(VK_ESCAPE, 0)),
            Some(PickResult::Cancelled)
        );
    }

    #[test]
    fn esc_key_up_does_not_cancel() {
        let mut p = HotkeyPicker::new(0.0);
        assert_eq!(p.on_key(raw(WM_KEYUP, VK_ESCAPE, 0)), None);
    }

    #[test]
    fn poll_times_out_at_15_seconds_from_the_start() {
        let mut p = HotkeyPicker::new(0.0);
        assert_eq!(p.poll(0.0), None);
        assert_eq!(p.poll(14.9), None);
        assert_eq!(p.poll(15.0), Some(PickResult::TimedOut));
        let mut p = HotkeyPicker::new(100.0);
        assert_eq!(p.poll(114.9), None);
        assert_eq!(p.poll(115.0), Some(PickResult::TimedOut));
        assert_eq!(PICK_TIMEOUT_SECS, 15.0);
    }

    #[test]
    fn nothing_comes_out_after_a_pick() {
        let mut p = HotkeyPicker::new(0.0);
        assert!(p.on_key(rctrl_down()).is_some());
        assert_eq!(p.on_key(key_down(0x7C, 0)), None);
        assert_eq!(p.on_key(key_down(VK_ESCAPE, 0)), None);
        assert_eq!(p.poll(60.0), None);
    }

    #[test]
    fn nothing_comes_out_after_a_cancel() {
        let mut p = HotkeyPicker::new(0.0);
        assert!(p.on_key(key_down(VK_ESCAPE, 0)).is_some());
        assert_eq!(p.on_key(rctrl_down()), None);
        assert_eq!(p.poll(60.0), None);
    }

    #[test]
    fn nothing_comes_out_after_a_timeout() {
        let mut p = HotkeyPicker::new(0.0);
        assert_eq!(p.poll(15.0), Some(PickResult::TimedOut));
        assert_eq!(p.poll(16.0), None);
        assert_eq!(p.on_key(rctrl_down()), None);
        assert_eq!(p.on_key(key_down(VK_ESCAPE, 0)), None);
    }
}
