//! The target of a dictation: what the foreground window and focused field looked like when
//! recording started. Mirrors `TargetContext.swift` and `ContextProbe.swift`. The UIA and Win32
//! reads live in the app crate and hand their raw values to the pure functions here.

use unicode_segmentation::UnicodeSegmentation;

/// Limits of ContextProbe.swift:35-44, counted in grapheme clusters.
pub const MAX_VALUE_GRAPHEMES: usize = 50_000;
pub const MAX_SELECTION_GRAPHEMES: usize = 10_000;
pub const CARET_TAIL_GRAPHEMES: usize = 500;

/// TargetContext.swift:4-12. `app_id` is the exe file name where macOS has the bundle id.
/// `password` is `None` when the flag could not be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetContext {
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub text_before_caret: Option<String>,
    pub selected_text: Option<String>,
    pub has_editable_focus: bool,
    pub password: Option<bool>,
}

/// The foreground app, read without UIA.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppInfo {
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

/// The focused element as UIA reported it. `caret` is a grapheme index into `value`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FocusRead {
    pub is_password: Option<bool>,
    pub editable: bool,
    pub value: Option<String>,
    pub caret: Option<usize>,
    pub selection: Option<String>,
}

/// Text is read only when the password flag was read and is false (spec README.md:64).
pub fn may_read_text(is_password: Option<bool>) -> bool {
    is_password == Some(false)
}

/// The last 500 graphemes before the caret; the end of the value when there is no caret. `None`
/// when the value has 50000 graphemes or more (ContextProbe.swift:35-41).
pub fn text_before_caret(value: &str, caret: Option<usize>) -> Option<String> {
    let all: Vec<&str> = value.graphemes(true).collect();
    if all.len() >= MAX_VALUE_GRAPHEMES {
        return None;
    }
    let end = caret.map_or(all.len(), |c| c.min(all.len()));
    let start = end.saturating_sub(CARET_TAIL_GRAPHEMES);
    Some(all[start..end].concat())
}

/// `None` when empty or 10000 graphemes or more (ContextProbe.swift:43).
pub fn selection_text(selected: &str) -> Option<String> {
    if selected.is_empty() || selected.graphemes(true).count() >= MAX_SELECTION_GRAPHEMES {
        return None;
    }
    Some(selected.to_string())
}

/// Joins the app and the focused element. No text for a password field or an unknown flag.
pub fn build_context(app: AppInfo, focus: Option<FocusRead>) -> TargetContext {
    let mut ctx = TargetContext {
        app_id: app.app_id,
        app_name: app.app_name,
        window_title: app.window_title,
        ..TargetContext::default()
    };
    if let Some(f) = focus {
        ctx.password = f.is_password;
        ctx.has_editable_focus = f.editable;
        if may_read_text(f.is_password) {
            ctx.text_before_caret = f
                .value
                .as_deref()
                .and_then(|v| text_before_caret(v, f.caret));
            ctx.selected_text = f.selection.as_deref().and_then(selection_text);
        }
    }
    ctx
}

/// File name of a full path, either separator. `None` for an empty name.
pub fn exe_file_name(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next().unwrap_or("");
    (!name.is_empty()).then(|| name.to_string())
}

/// The version-info FileDescription when it has text, else the exe name without `.exe`.
pub fn app_name(file_description: Option<&str>, exe_file_name: &str) -> String {
    match file_description.map(str::trim) {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => {
            let n = exe_file_name.len();
            if n >= 4
                && exe_file_name.is_char_boundary(n - 4)
                && exe_file_name[n - 4..].eq_ignore_ascii_case(".exe")
            {
                exe_file_name[..n - 4].to_string()
            } else {
                exe_file_name.to_string()
            }
        }
    }
}

/// VerQueryValue sub-block: `\StringFileInfo\<lang 4 hex><codepage 4 hex>\FileDescription`,
/// upper-case hex.
pub fn file_description_query(lang: u16, codepage: u16) -> String {
    format!(r"\StringFileInfo\{lang:04X}{codepage:04X}\FileDescription")
}

/// UIA control types Edit 50004, Document 50030 and ComboBox 50003, or any password field
/// (ContextProbe.swift:30-31).
pub fn is_editable(control_type: i32, is_password: bool) -> bool {
    is_password || matches!(control_type, 50004 | 50030 | 50003)
}

/// UWP apps run under ApplicationFrameHost.exe; the real process is a child window's.
pub fn is_frame_host(exe_file_name: &str) -> bool {
    exe_file_name.eq_ignore_ascii_case("ApplicationFrameHost.exe")
}

/// Debug log line: which fields exist, never their values.
pub fn presence_flags_line(ctx: &TargetContext) -> String {
    let yn = |b: bool| if b { "yes" } else { "no" };
    let password = match ctx.password {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    format!(
        "context: app_id={} app_name={} title={} before_caret={} selection={} editable={} password={}",
        yn(ctx.app_id.is_some()),
        yn(ctx.app_name.is_some()),
        yn(ctx.window_title.is_some()),
        yn(ctx.text_before_caret.is_some()),
        yn(ctx.selected_text.is_some()),
        yn(ctx.has_editable_focus),
        password
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;

    fn graphemes(s: &str) -> usize {
        s.graphemes(true).count()
    }

    fn app() -> AppInfo {
        AppInfo {
            app_id: Some("notepad.exe".into()),
            app_name: Some("Notepad".into()),
            window_title: Some("notes - Notepad".into()),
        }
    }

    fn focus(is_password: Option<bool>) -> FocusRead {
        FocusRead {
            is_password,
            editable: true,
            value: Some("secret value".into()),
            caret: Some(6),
            selection: Some("secret".into()),
        }
    }

    #[test]
    fn limits_match_macos() {
        assert_eq!(MAX_VALUE_GRAPHEMES, 50_000);
        assert_eq!(MAX_SELECTION_GRAPHEMES, 10_000);
        assert_eq!(CARET_TAIL_GRAPHEMES, 500);
    }

    #[test]
    fn a_value_of_49999_graphemes_is_kept_and_50000_is_dropped() {
        assert!(text_before_caret(&"a".repeat(49_999), None).is_some());
        assert_eq!(text_before_caret(&"a".repeat(50_000), None), None);
    }

    #[test]
    fn the_value_limit_counts_graphemes_not_scalars() {
        // 30000 clusters of two scalars each: 60000 scalars, under the limit in graphemes.
        let value = "e\u{301}".repeat(30_000);
        assert_eq!(graphemes(&value), 30_000);
        assert!(text_before_caret(&value, None).is_some());
    }

    #[test]
    fn a_selection_of_9999_graphemes_is_kept_and_10000_is_dropped() {
        assert!(selection_text(&"a".repeat(9_999)).is_some());
        assert_eq!(selection_text(&"a".repeat(10_000)), None);
    }

    #[test]
    fn an_empty_selection_is_dropped() {
        assert_eq!(selection_text(""), None);
        assert_eq!(selection_text("x").as_deref(), Some("x"));
    }

    #[test]
    fn the_tail_before_the_caret_is_the_last_500_graphemes() {
        let value = format!("{}{}", "a".repeat(100), "b".repeat(600));
        let out = text_before_caret(&value, Some(700)).unwrap();
        assert_eq!(out, "b".repeat(500));
    }

    #[test]
    fn text_after_the_caret_is_not_included() {
        let out = text_before_caret("hello world", Some(5)).unwrap();
        assert_eq!(out, "hello");
    }

    #[test]
    fn a_caret_past_the_end_is_clamped() {
        let out = text_before_caret("abc", Some(99)).unwrap();
        assert_eq!(out, "abc");
    }

    #[test]
    fn no_caret_takes_the_end_of_the_value() {
        let value = format!("{}{}", "a".repeat(100), "b".repeat(500));
        assert_eq!(text_before_caret(&value, None).unwrap(), "b".repeat(500));
    }

    #[test]
    fn the_tail_does_not_split_a_grapheme_cluster() {
        let value = "e\u{301}".repeat(600);
        let out = text_before_caret(&value, None).unwrap();
        assert_eq!(graphemes(&out), 500);
        assert!(out.starts_with('e'));
    }

    #[test]
    fn only_a_false_password_flag_allows_reading_text() {
        assert!(may_read_text(Some(false)));
        assert!(!may_read_text(Some(true)));
        assert!(!may_read_text(None));
    }

    #[test]
    fn a_password_field_gives_a_context_without_text() {
        let ctx = build_context(app(), Some(focus(Some(true))));
        assert_eq!(ctx.password, Some(true));
        assert_eq!(ctx.text_before_caret, None);
        assert_eq!(ctx.selected_text, None);
    }

    #[test]
    fn unknown_password_flag_reads_no_text() {
        let ctx = build_context(app(), Some(focus(None)));
        assert_eq!(ctx.password, None);
        assert_eq!(ctx.text_before_caret, None);
        assert_eq!(ctx.selected_text, None);
    }

    #[test]
    fn a_readable_non_password_field_gives_text_and_selection() {
        let ctx = build_context(app(), Some(focus(Some(false))));
        assert_eq!(ctx.password, Some(false));
        assert_eq!(ctx.text_before_caret.as_deref(), Some("secret"));
        assert_eq!(ctx.selected_text.as_deref(), Some("secret"));
        assert!(ctx.has_editable_focus);
    }

    #[test]
    fn the_app_fields_pass_through_and_a_missing_focus_leaves_the_rest_empty() {
        let ctx = build_context(app(), None);
        assert_eq!(ctx.app_id.as_deref(), Some("notepad.exe"));
        assert_eq!(ctx.app_name.as_deref(), Some("Notepad"));
        assert_eq!(ctx.window_title.as_deref(), Some("notes - Notepad"));
        assert_eq!(ctx.password, None);
        assert_eq!(ctx.text_before_caret, None);
        assert!(!ctx.has_editable_focus);
    }

    #[test]
    fn exe_file_name_takes_the_last_path_component() {
        let n = |p| exe_file_name(p).unwrap();
        assert_eq!(
            n(r"C:\Program Files\Notepad++\notepad++.exe"),
            "notepad++.exe"
        );
        assert_eq!(n("C:/Windows/notepad.exe"), "notepad.exe");
        assert_eq!(n("code.exe"), "code.exe");
    }

    #[test]
    fn exe_file_name_is_none_for_an_empty_name() {
        assert_eq!(exe_file_name(""), None);
        assert_eq!(exe_file_name(r"C:\Windows\"), None);
    }

    #[test]
    fn app_name_prefers_the_file_description() {
        assert_eq!(
            app_name(Some("Windows Terminal"), "WindowsTerminal.exe"),
            "Windows Terminal"
        );
    }

    #[test]
    fn app_name_falls_back_to_the_exe_name_without_extension() {
        assert_eq!(app_name(None, "notepad.exe"), "notepad");
        assert_eq!(app_name(Some(""), "notepad.exe"), "notepad");
        assert_eq!(app_name(Some("   "), "Code.EXE"), "Code");
    }

    #[test]
    fn version_query_uses_upper_case_hex_for_language_and_codepage() {
        assert_eq!(
            file_description_query(0x0409, 0x04B0),
            r"\StringFileInfo\040904B0\FileDescription"
        );
        assert_eq!(
            file_description_query(0x0422, 0x04E4),
            r"\StringFileInfo\042204E4\FileDescription"
        );
        assert_eq!(
            file_description_query(0x040a, 0x04b0),
            r"\StringFileInfo\040A04B0\FileDescription"
        );
    }

    #[test]
    fn edit_document_and_combo_box_are_editable() {
        for t in [50004, 50030, 50003] {
            assert!(is_editable(t, false), "{t}");
        }
    }

    #[test]
    fn other_controls_are_not_editable_unless_a_password() {
        // Button 50000, Text 50020, Pane 50033
        for t in [50000, 50020, 50033, 0] {
            assert!(!is_editable(t, false), "{t}");
            assert!(is_editable(t, true), "{t}");
        }
    }

    #[test]
    fn frame_host_is_matched_ignoring_case() {
        assert!(is_frame_host("ApplicationFrameHost.exe"));
        assert!(is_frame_host("applicationframehost.exe"));
        assert!(!is_frame_host("notepad.exe"));
        assert!(!is_frame_host("ApplicationFrameHost"));
    }

    #[test]
    fn the_presence_line_has_flags_and_no_field_values() {
        let ctx = TargetContext {
            app_id: Some("zzappid.exe".into()),
            app_name: Some("ZZAppName".into()),
            window_title: Some("ZZTitle".into()),
            text_before_caret: Some("ZZBefore".into()),
            selected_text: Some("ZZSelected".into()),
            has_editable_focus: true,
            password: Some(false),
        };
        let line = presence_flags_line(&ctx);
        assert!(!line.to_lowercase().contains("zz"), "{line}");
        assert_eq!(
            line,
            "context: app_id=yes app_name=yes title=yes before_caret=yes selection=yes editable=yes password=no"
        );
    }

    #[test]
    fn the_presence_line_marks_missing_fields_and_an_unknown_flag() {
        let line = presence_flags_line(&TargetContext::default());
        assert_eq!(
            line,
            "context: app_id=no app_name=no title=no before_caret=no selection=no editable=no password=unknown"
        );
        let pw = TargetContext {
            password: Some(true),
            ..TargetContext::default()
        };
        assert!(presence_flags_line(&pw).ends_with("password=yes"));
    }
}
