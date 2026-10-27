//! Pure decisions of the paste insertion. Mirrors `PasteInserter.swift`; the Win32 calls live in the
//! app crate. Nothing here touches the clipboard.

/// Wait between the paste keys and the clipboard restore (PasteInserter.swift:52).
pub const RESTORE_DELAY_MS: u64 = 350;

use crate::keys::{LLKHF_EXTENDED, LLKHF_INJECTED};

// Generic VK_CONTROL (not VK_LCONTROL) with scan code 0x1D and no extended flag is the left key.
const VK_LCONTROL_PASTE: u16 = 0x11;
const VK_V: u16 = 0x56;
const SCAN_CTRL: u16 = 0x1D;
const SCAN_V: u16 = 0x2F;

/// PasteInserter.swift:19 (.secureInput).
pub const PASSWORD_REFUSED: &str = "A password field is active; nothing was inserted";
/// PasteInserter.swift:20 (.empty).
pub const NOTHING_TO_INSERT: &str = "Nothing to insert";
/// OpenClipboard still failing after the retries.
pub const CLIPBOARD_BUSY: &str = "The clipboard is in use by another app; nothing was inserted";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertError {
    Empty,
    Password,
}

impl std::fmt::Display for InsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InsertError::Empty => f.write_str(NOTHING_TO_INSERT),
            InsertError::Password => f.write_str(PASSWORD_REFUSED),
        }
    }
}

/// How an accepted insert ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Pasted,
    /// The keys could not be sent; the text stays on the clipboard (PasteInserter.swift:66-69).
    ClipboardOnly,
}

/// Empty text is refused first, then a password field. `captured` is the flag read at recording
/// start, `now` the one read just before the paste; `None` means unreadable and pastes anyway
/// (ContextProbe.swift:24, PasteInserter.swift:58).
pub fn decide(text: &str, captured: Option<bool>, now: Option<bool>) -> Result<(), InsertError> {
    if text.is_empty() {
        return Err(InsertError::Empty);
    }
    if captured == Some(true) || now == Some(true) {
        return Err(InsertError::Password);
    }
    Ok(())
}

/// True for formats worth saving across the paste. False for GDI handles and private ranges that
/// cannot be copied as plain memory: CF_BITMAP 2, CF_METAFILEPICT 3, CF_PALETTE 9,
/// CF_ENHMETAFILE 14, CF_OWNERDISPLAY 0x80, CF_DSPBITMAP 0x82, CF_DSPMETAFILEPICT 0x83,
/// CF_DSPENHMETAFILE 0x8E, 0x200-0x2FF and 0x300-0x3FF.
pub fn is_restorable_format(format: u32) -> bool {
    !matches!(
        format,
        2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x200..=0x3FF
    )
}

/// One keyboard event of the paste chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteKey {
    pub vk: u16,
    pub scan: u16,
    pub key_up: bool,
    /// Would set KEYEVENTF_EXTENDEDKEY, which turns Ctrl into Right Ctrl, our hotkey.
    pub extended: bool,
}

impl PasteKey {
    /// The `LLKHF_*` flags a low-level hook would see for this event, injected bit included.
    pub fn hook_flags(&self, injected: bool) -> u32 {
        let mut flags = 0;
        if self.extended {
            flags |= LLKHF_EXTENDED;
        }
        if injected {
            flags |= LLKHF_INJECTED;
        }
        flags
    }
}

/// Ctrl down, V down, V up, Ctrl up, sent in one SendInput call.
pub fn paste_keys() -> [PasteKey; 4] {
    let key = |vk, scan, key_up| PasteKey {
        vk,
        scan,
        key_up,
        extended: false,
    };
    [
        key(VK_LCONTROL_PASTE, SCAN_CTRL, false),
        key(VK_V, SCAN_V, false),
        key(VK_V, SCAN_V, true),
        key(VK_LCONTROL_PASTE, SCAN_CTRL, true),
    ]
}

/// Widget text when the text stays on the clipboard (DictationSession.swift:220).
pub fn clipboard_only_message(preview: &str) -> String {
    format!("Copied (press Ctrl+V to paste): {preview}")
}

/// Windows mandatory integrity levels (RIDs of the integrity SID): the target is only compared
/// by these numbers, so the pure decision does not need the Win32 types.
pub const INTEGRITY_MEDIUM: u32 = 0x2000;
pub const INTEGRITY_HIGH: u32 = 0x3000;

/// Whether the paste keys may go to the foreground window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    SendKeys,
    /// UIPI drops keys sent to a higher-integrity window, so leave the text on the clipboard.
    ClipboardOnly,
}

/// `ours` and `target` are integrity RIDs, `None` when unreadable. A target that cannot be read is
/// treated as elevated: opening it is exactly what a higher-integrity process refuses.
pub fn paste_mode(ours: Option<u32>, target: Option<u32>) -> PasteMode {
    match (ours, target) {
        (Some(ours), Some(target)) if target <= ours => PasteMode::SendKeys,
        _ => PasteMode::ClipboardOnly,
    }
}

/// Registered clipboard formats that keep the dictated text out of Windows clipboard history,
/// cloud sync and clipboard monitors: (name, DWORD value, `None` for the empty marker). Written
/// with the text in the same clipboard transaction, never with the restored content.
pub const CLIPBOARD_PRIVACY_MARKERS: [(&str, Option<u32>); 3] = [
    ("ExcludeClipboardContentFromMonitorProcessing", None),
    ("CanIncludeInClipboardHistory", Some(0)),
    ("CanUploadToCloudClipboard", Some(0)),
];

/// capture_context stops making UI Automation calls after this long in total.
pub const CAPTURE_DEADLINE_MS: u64 = 1500;

/// True while `elapsed_ms` is still inside the capture deadline.
pub fn within_capture_deadline(elapsed_ms: u64) -> bool {
    elapsed_ms < CAPTURE_DEADLINE_MS
}

/// The document is read up to this many UTF-16 units; macOS keeps no context from 50000 on
/// (ContextProbe.swift:35-41), one extra unit tells "exactly 50000" from "longer".
pub const DOCUMENT_READ_UNITS: usize = 50_001;

/// True when a document read of `units_read` units reached the limit, so no context is kept.
pub fn document_too_long(units_read: usize) -> bool {
    units_read >= DOCUMENT_READ_UNITS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{
        is_accepted, KeyFilter, RawKey, LLKHF_EXTENDED, LLKHF_INJECTED, VK_CONTROL, VK_RCONTROL,
        WM_KEYDOWN, WM_KEYUP,
    };

    const COMBOS: [Option<bool>; 3] = [Some(true), Some(false), None];

    #[test]
    fn restore_delay_is_350_ms() {
        assert_eq!(RESTORE_DELAY_MS, 350);
    }

    #[test]
    fn refuses_when_the_field_was_a_password_at_start() {
        for now in COMBOS {
            assert_eq!(
                decide("hello", Some(true), now),
                Err(InsertError::Password),
                "now={now:?}"
            );
        }
    }

    #[test]
    fn refuses_when_the_field_is_a_password_now() {
        for captured in COMBOS {
            assert_eq!(
                decide("hello", captured, Some(true)),
                Err(InsertError::Password),
                "captured={captured:?}"
            );
        }
    }

    #[test]
    fn inserts_when_neither_flag_says_password() {
        for captured in [Some(false), None] {
            for now in [Some(false), None] {
                assert_eq!(
                    decide("hello", captured, now),
                    Ok(()),
                    "{captured:?} {now:?}"
                );
            }
        }
    }

    #[test]
    fn empty_text_is_refused_for_every_flag_combination() {
        for captured in COMBOS {
            for now in COMBOS {
                assert_eq!(decide("", captured, now), Err(InsertError::Empty));
            }
        }
    }

    #[test]
    fn error_texts_match_macos() {
        assert_eq!(
            InsertError::Password.to_string(),
            "A password field is active; nothing was inserted"
        );
        assert_eq!(InsertError::Empty.to_string(), "Nothing to insert");
        assert_eq!(
            CLIPBOARD_BUSY,
            "The clipboard is in use by another app; nothing was inserted"
        );
    }

    #[test]
    fn text_and_unicode_text_formats_are_restorable() {
        for f in [1, 7, 13, 16] {
            assert!(is_restorable_format(f), "{f}");
        }
    }

    #[test]
    fn dib_hdrop_and_registered_formats_are_restorable() {
        for f in [8, 15, 17, 0xC000, 0xC123, 0xFFFF] {
            assert!(is_restorable_format(f), "{f:#x}");
        }
    }

    #[test]
    fn gdi_handle_formats_are_skipped() {
        for f in [2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E] {
            assert!(!is_restorable_format(f), "{f:#x}");
        }
    }

    #[test]
    fn private_ranges_are_skipped_at_both_edges() {
        for f in [0x200, 0x2FF, 0x300, 0x3FF] {
            assert!(!is_restorable_format(f), "{f:#x}");
        }
        for f in [0x1FF, 0x400] {
            assert!(is_restorable_format(f), "{f:#x}");
        }
    }

    #[test]
    fn paste_chord_is_ctrl_down_v_down_v_up_ctrl_up() {
        let k = paste_keys();
        let shape: Vec<(u16, bool)> = k.iter().map(|k| (k.vk, k.key_up)).collect();
        assert_eq!(
            shape,
            [(0x11, false), (0x56, false), (0x56, true), (0x11, true)]
        );
    }

    #[test]
    fn paste_keys_carry_scan_codes_and_never_the_extended_flag() {
        let k = paste_keys();
        assert_eq!([k[0].scan, k[1].scan], [0x1D, 0x2F]);
        assert!(k.iter().all(|k| k.scan != 0));
        assert!(k.iter().all(|k| !k.extended));
    }

    #[test]
    fn hook_flags_follow_the_extended_and_injected_bits() {
        let plain = PasteKey {
            vk: 0x56,
            scan: 0x2F,
            key_up: false,
            extended: false,
        };
        let ext = PasteKey {
            extended: true,
            ..plain
        };
        assert_eq!(plain.hook_flags(false), 0);
        assert_eq!(plain.hook_flags(true), LLKHF_INJECTED);
        assert_eq!(ext.hook_flags(false), LLKHF_EXTENDED);
        assert_eq!(ext.hook_flags(true), LLKHF_EXTENDED | LLKHF_INJECTED);
    }

    #[test]
    fn the_hook_accepts_none_of_the_paste_keys() {
        for k in paste_keys() {
            for injected in [false, true] {
                assert!(
                    !is_accepted(k.vk as u32, k.hook_flags(injected), VK_RCONTROL),
                    "{k:?}"
                );
            }
        }
    }

    #[test]
    fn the_key_filter_drops_every_injected_paste_key() {
        let mut f = KeyFilter::new();
        for k in paste_keys() {
            let message = if k.key_up { WM_KEYUP } else { WM_KEYDOWN };
            let raw = RawKey {
                message,
                vk: k.vk as u32,
                flags: k.hook_flags(true),
            };
            assert_eq!(f.filter(raw), None, "{k:?}");
        }
    }

    #[test]
    fn ctrl_in_the_chord_is_the_left_one() {
        // A Right Ctrl look-alike would start a recording from our own paste.
        let ctrl = paste_keys()[0];
        assert_eq!(ctrl.vk as u32, VK_CONTROL);
        assert_eq!(ctrl.hook_flags(false) & LLKHF_EXTENDED, 0);
    }

    #[test]
    fn clipboard_only_message_names_the_paste_shortcut() {
        assert_eq!(
            clipboard_only_message("hello there"),
            "Copied (press Ctrl+V to paste): hello there"
        );
    }

    #[test]
    fn a_higher_integrity_target_gets_no_keys() {
        assert_eq!(
            paste_mode(Some(INTEGRITY_MEDIUM), Some(INTEGRITY_HIGH)),
            PasteMode::ClipboardOnly
        );
    }

    #[test]
    fn an_equal_or_lower_integrity_target_gets_the_paste() {
        for target in [INTEGRITY_MEDIUM, 0x1000, 0] {
            assert_eq!(
                paste_mode(Some(INTEGRITY_MEDIUM), Some(target)),
                PasteMode::SendKeys,
                "{target:#x}"
            );
        }
        assert_eq!(
            paste_mode(Some(INTEGRITY_HIGH), Some(INTEGRITY_HIGH)),
            PasteMode::SendKeys
        );
    }

    #[test]
    fn an_unknown_integrity_means_clipboard_only() {
        for (ours, target) in [
            (Some(INTEGRITY_MEDIUM), None),
            (None, Some(INTEGRITY_MEDIUM)),
            (None, None),
        ] {
            assert_eq!(paste_mode(ours, target), PasteMode::ClipboardOnly);
        }
    }

    #[test]
    fn the_privacy_markers_are_the_three_documented_formats() {
        assert_eq!(
            CLIPBOARD_PRIVACY_MARKERS,
            [
                ("ExcludeClipboardContentFromMonitorProcessing", None),
                ("CanIncludeInClipboardHistory", Some(0)),
                ("CanUploadToCloudClipboard", Some(0)),
            ]
        );
    }

    #[test]
    fn the_capture_deadline_is_1500_ms() {
        assert!(within_capture_deadline(0));
        assert!(within_capture_deadline(1499));
        assert!(!within_capture_deadline(1500));
        assert!(!within_capture_deadline(9000));
    }

    #[test]
    fn a_document_read_that_hits_the_limit_is_too_long() {
        assert!(!document_too_long(0));
        assert!(!document_too_long(50_000));
        assert!(document_too_long(50_001));
        assert!(document_too_long(60_000));
    }
}
