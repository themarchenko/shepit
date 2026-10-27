//! Target profiles and mode resolution. Mirrors `Profile.swift` in the Swift core.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Clean,
    Prompt,
    Raw,
}

impl Mode {
    /// Parses the fixture and settings string: `raw`, `clean` or `prompt`.
    pub fn from_id(id: &str) -> Option<Mode> {
        match id {
            "clean" => Some(Mode::Clean),
            "prompt" => Some(Mode::Prompt),
            "raw" => Some(Mode::Raw),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Clean => "clean",
            Mode::Prompt => "prompt",
            Mode::Raw => "raw",
        }
    }
}

/// The Swift `Profile` also carries `name` and `styleRules`; those only feed the formatter prompt,
/// which is out of scope here.
#[derive(Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: &'static str,
    pub is_ai_target: bool,
    pub allows_markdown: bool,
    pub strip_trailing_newline: bool,
}

// Profile.swift:40-91 (BuiltInProfiles). stripTrailingNewline defaults to true (Profile.swift:30)
// and no built-in overrides it.
const fn profile(id: &'static str, is_ai_target: bool, allows_markdown: bool) -> Profile {
    Profile {
        id,
        is_ai_target,
        allows_markdown,
        strip_trailing_newline: true,
    }
}

static CLAUDE_CODE_TERMINAL: Profile = profile("claude-code-terminal", true, false);
static TERMINAL: Profile = profile("terminal", false, false);
static AI_CHAT: Profile = profile("ai-chat", true, true);
static CODE_EDITOR: Profile = profile("code-editor", true, true);
static MESSENGER: Profile = profile("messenger", false, false);
static MAIL: Profile = profile("mail", false, false);
static NOTES: Profile = profile("notes", false, true);
static GENERIC: Profile = profile("generic", false, false);

static ALL: [&Profile; 8] = [
    &CLAUDE_CODE_TERMINAL,
    &TERMINAL,
    &AI_CHAT,
    &CODE_EDITOR,
    &MESSENGER,
    &MAIL,
    &NOTES,
    &GENERIC,
];

/// Looks up a built-in profile by id (`BuiltInProfiles.byID`).
pub fn profile_by_id(id: &str) -> Option<&'static Profile> {
    ALL.iter().copied().find(|p| p.id == id)
}

// Profile.swift:96-109 (ProfileResolver tables). Bundle ids match exactly and case-sensitively.
const TERMINALS: [&str; 7] = [
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.Warp-Stable",
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "org.alacritty",
];
const AI_APPS: [&str; 3] = [
    "com.anthropic.claudefordesktop",
    "com.openai.chat",
    "com.openai.codex",
];
const AI_HOSTS: [&str; 6] = [
    "claude.ai",
    "chatgpt.com",
    "chat.openai.com",
    "gemini.google.com",
    "perplexity.ai",
    "copilot.microsoft.com",
];
const CODE_EDITORS: [&str; 4] = [
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92",
    "com.apple.dt.Xcode",
    "dev.zed.Zed",
];
const MESSENGERS: [&str; 7] = [
    "ru.keepcoder.Telegram",
    "com.tdesktop.Telegram",
    "com.tinyspeck.slackmacgap",
    "com.apple.MobileSMS",
    "com.hnc.Discord",
    "net.whatsapp.WhatsApp",
    "com.facebook.archon",
];
const MAIL_APPS: [&str; 3] = [
    "com.apple.mail",
    "com.readdle.smartemail-Mac",
    "com.superhuman.electron",
];
const NOTE_APPS: [&str; 6] = [
    "com.apple.Notes",
    "md.obsidian",
    "notion.id",
    "com.apple.TextEdit",
    "com.apple.iWork.Pages",
    "com.bear-writer.bear",
];
const MAIL_HOSTS: [&str; 3] = ["mail.google.com", "outlook.live.com", "outlook.office.com"];
const DOC_HOSTS: [&str; 2] = ["docs.google.com", "notion.so"];

fn host_matches(host: &str, base: &str) -> bool {
    host == base
        || host
            .strip_suffix(base)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// Profile.swift:111 `ProfileResolver.resolve`.
///
/// Parity notes. `str::to_lowercase` stands in for `lowercased()` on the window title, then a plain
/// substring test for "claude" (Swift `contains` may also match canonically equivalent sequences,
/// irrelevant for ASCII "claude" except exotic decomposed input).
pub fn resolve_profile(
    bundle_id: Option<&str>,
    window_title: Option<&str>,
    url: Option<&str>,
) -> &'static Profile {
    let bundle = bundle_id.unwrap_or("");
    let title = window_title.unwrap_or("").to_lowercase();
    if TERMINALS.contains(&bundle) {
        return if title.contains("claude") {
            &CLAUDE_CODE_TERMINAL
        } else {
            &TERMINAL
        };
    }
    if AI_APPS.contains(&bundle) {
        return &AI_CHAT;
    }
    if CODE_EDITORS.contains(&bundle) {
        return &CODE_EDITOR;
    }
    if MESSENGERS.contains(&bundle) {
        return &MESSENGER;
    }
    if MAIL_APPS.contains(&bundle) {
        return &MAIL;
    }
    if NOTE_APPS.contains(&bundle) {
        return &NOTES;
    }
    resolve_by_host(url)
}

fn resolve_by_host(url: Option<&str>) -> &'static Profile {
    if let Some(host) = host_of(url) {
        if AI_HOSTS.iter().any(|b| host_matches(&host, b)) {
            return &AI_CHAT;
        }
        // Mail hosts match exactly, no subdomains (Profile.swift:125).
        if MAIL_HOSTS.contains(&host.as_str()) {
            return &MAIL;
        }
        if DOC_HOSTS.iter().any(|b| host_matches(&host, b)) {
            return &NOTES;
        }
    }
    &GENERIC
}

/// Profile.swift:134 `hostOf`: lowercased host with a leading "www." removed.
///
/// Swift uses `URL(string:)?.host`. This is a small RFC 3986 authority parse, not a full URL parser,
/// pinned to what Foundation returns on the `Crafted/*` fixture cases. The authority ends at the first
/// `/ ? #` (not at a backslash: Foundation keeps a backslash in the userinfo, so
/// `https://evil.com\@claude.ai/` has host `claude.ai`) and userinfo is dropped up to the last `@`.
/// A host with a backslash or any character outside `[a-z0-9.-]` is rejected, which also rejects
/// IPv6 literals (never in a host table). Not guaranteed: percent-encoded or IDN hosts, bad ports,
/// and other URLs Foundation rejects (only whitespace and control characters are rejected here).
/// Scheme-less input such as "claude.ai/new" has no host in both.
fn host_of(url: Option<&str>) -> Option<String> {
    let url = url?;
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let after_scheme = match url.find(':') {
        Some(i) if is_scheme(&url[..i]) => &url[i + 1..],
        _ => url,
    };
    let rest = after_scheme.strip_prefix("//")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let hostport = authority.rsplit('@').next().unwrap_or("");
    let host = hostport.split(':').next().unwrap_or("").to_lowercase();
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return None;
    }
    Some(
        host.strip_prefix("www.")
            .map(str::to_string)
            .unwrap_or(host),
    )
}

/// RFC 3986 scheme: a letter, then letters, digits, `+`, `-` or `.`.
fn is_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
}

const WIN_TERMINALS: [&str; 8] = [
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
];
const WIN_AI_APPS: [&str; 3] = ["claude.exe", "chatgpt.exe", "codex.exe"];
const WIN_CODE_EDITORS: [&str; 3] = ["code.exe", "cursor.exe", "zed.exe"];
const WIN_MESSENGERS: [&str; 5] = [
    "telegram.exe",
    "slack.exe",
    "discord.exe",
    "whatsapp.exe",
    "messenger.exe",
];
const WIN_MAIL_APPS: [&str; 3] = ["outlook.exe", "olk.exe", "superhuman.exe"];
const WIN_NOTE_APPS: [&str; 5] = [
    "notepad.exe",
    "winword.exe",
    "obsidian.exe",
    "notion.exe",
    "onenote.exe",
];

/// Windows identity: the exe file name stands in for the bundle id. Matched ignoring case, with the
/// same order and host rules as `resolve_profile`. Spec profiles-modes.md "Windows app identifiers".
pub fn resolve_exe_profile(
    exe: Option<&str>,
    window_title: Option<&str>,
    url: Option<&str>,
) -> &'static Profile {
    let exe = exe.unwrap_or("").to_lowercase();
    let title = window_title.unwrap_or("").to_lowercase();
    if WIN_TERMINALS.contains(&exe.as_str()) {
        return if title.contains("claude") {
            &CLAUDE_CODE_TERMINAL
        } else {
            &TERMINAL
        };
    }
    for (p, names) in [
        (&AI_CHAT, &WIN_AI_APPS[..]),
        (&CODE_EDITOR, &WIN_CODE_EDITORS[..]),
        (&MESSENGER, &WIN_MESSENGERS[..]),
        (&MAIL, &WIN_MAIL_APPS[..]),
        (&NOTES, &WIN_NOTE_APPS[..]),
    ] {
        if names.contains(&exe.as_str()) {
            return p;
        }
    }
    resolve_by_host(url)
}

/// The exe tables as `(profile id, lowercase exe names)`. A terminal is `terminal`, or
/// `claude-code-terminal` when the title says claude.
pub fn exe_tables() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("terminal", &WIN_TERMINALS[..]),
        ("ai-chat", &WIN_AI_APPS[..]),
        ("code-editor", &WIN_CODE_EDITORS[..]),
        ("messenger", &WIN_MESSENGERS[..]),
        ("mail", &WIN_MAIL_APPS[..]),
        ("notes", &WIN_NOTE_APPS[..]),
    ]
}

/// Profile.swift:145 `ModeResolver.resolve`: clean becomes prompt for AI targets once the dictation
/// reaches the word threshold.
pub fn resolve_mode(
    requested: Mode,
    profile: &Profile,
    word_count: usize,
    prompt_mode_min_words: usize,
) -> Mode {
    if requested == Mode::Clean && profile.is_ai_target && word_count >= prompt_mode_min_words {
        return Mode::Prompt;
    }
    requested
}

/// Profile.swift:150 `ModeResolver.wordCount`. Swift tests `Character.isWhitespace || isNewline`, which
/// look at the first scalar of each grapheme cluster (see `cleanup::words`).
pub fn word_count(text: &str) -> usize {
    crate::cleanup::words(text).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_parsing() {
        assert_eq!(
            host_of(Some("https://WWW.Claude.ai:443/x?y#z")).as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host_of(Some("https://user:pw@chatgpt.com/")).as_deref(),
            Some("chatgpt.com")
        );
        assert_eq!(host_of(Some("claude.ai/new")), None);
        assert_eq!(host_of(Some("https://bad host/")), None);
        assert_eq!(host_of(Some("https://evil.com\\.claude.ai/")), None);
        assert_eq!(
            host_of(Some("https://evil.com\\@claude.ai/")).as_deref(),
            Some("claude.ai")
        );
        assert_eq!(host_of(Some("1http://claude.ai/")), None);
        assert_eq!(host_of(None), None);
    }

    #[test]
    fn subdomains_and_lookalikes() {
        let p = |u| resolve_profile(Some("com.google.Chrome"), None, Some(u)).id;
        assert_eq!(p("https://app.claude.ai/"), "ai-chat");
        assert_eq!(p("https://notclaude.ai/"), "generic");
        assert_eq!(p("https://sub.mail.google.com/"), "generic");
        assert_eq!(p("https://x.docs.google.com/"), "notes");
    }

    #[test]
    fn mode_threshold_is_inclusive_and_only_for_clean_ai() {
        let ai = profile_by_id("ai-chat").unwrap();
        let msg = profile_by_id("messenger").unwrap();
        assert_eq!(resolve_mode(Mode::Clean, ai, 10, 10), Mode::Prompt);
        assert_eq!(resolve_mode(Mode::Clean, ai, 9, 10), Mode::Clean);
        assert_eq!(resolve_mode(Mode::Raw, ai, 99, 10), Mode::Raw);
        assert_eq!(resolve_mode(Mode::Clean, msg, 99, 10), Mode::Clean);
    }

    #[test]
    fn counts_words_across_whitespace_kinds() {
        assert_eq!(word_count(" a\u{2028}b\u{a0}c\r\nd "), 4);
        assert_eq!(word_count(""), 0);
    }

    // Windows identity.

    fn exe(name: &str, title: Option<&str>) -> &'static str {
        resolve_exe_profile(Some(name), title, None).id
    }

    #[test]
    fn every_listed_exe_resolves_to_its_table_profile() {
        for (id, names) in exe_tables() {
            for name in names {
                let got = exe(name, None);
                let want = if id == "claude-code-terminal" {
                    "terminal"
                } else {
                    id
                };
                assert_eq!(got, want, "{name}");
            }
        }
    }

    #[test]
    fn exe_matching_ignores_case() {
        assert_eq!(exe("WindowsTerminal.exe", None), "terminal");
        assert_eq!(exe("CODE.EXE", None), "code-editor");
        assert_eq!(exe("Slack.exe", None), "messenger");
    }

    #[test]
    fn a_terminal_with_claude_in_the_title_is_claude_code() {
        assert_eq!(
            exe("pwsh.exe", Some("✳ Claude Code")),
            "claude-code-terminal"
        );
        assert_eq!(exe("cmd.exe", Some("CLAUDE")), "claude-code-terminal");
        assert_eq!(exe("pwsh.exe", Some("pwsh")), "terminal");
    }

    #[test]
    fn a_claude_title_does_not_change_a_non_terminal() {
        assert_eq!(exe("notepad.exe", Some("claude notes")), "notes");
    }

    #[test]
    fn unknown_and_missing_exe_are_generic() {
        assert_eq!(exe("calc.exe", None), "generic");
        assert_eq!(resolve_exe_profile(None, None, None).id, "generic");
    }

    #[test]
    fn macos_bundle_ids_are_generic_here() {
        assert_eq!(exe("com.apple.Terminal", None), "generic");
    }

    #[test]
    fn a_browser_exe_falls_through_to_the_url_host_rules() {
        let p = |u| resolve_exe_profile(Some("chrome.exe"), None, Some(u)).id;
        assert_eq!(p("https://claude.ai/new"), "ai-chat");
        assert_eq!(p("https://mail.google.com/"), "mail");
        assert_eq!(p("https://sub.mail.google.com/"), "generic");
        assert_eq!(p("https://docs.google.com/d/1"), "notes");
        assert_eq!(p("https://example.com/"), "generic");
    }

    #[test]
    fn exe_tables_are_lowercase_exe_names_and_disjoint() {
        let mut seen = std::collections::HashSet::new();
        let tables = exe_tables();
        assert!(!tables.is_empty());
        for (id, names) in tables {
            for name in names {
                assert_eq!(*name, name.to_lowercase(), "{id}");
                assert!(name.ends_with(".exe"), "{name}");
                assert!(seen.insert(*name), "{name} is listed twice");
            }
        }
    }

    #[test]
    fn exe_tables_cover_the_planned_profiles() {
        let ids: Vec<&str> = exe_tables().iter().map(|(id, _)| *id).collect();
        for want in [
            "terminal",
            "ai-chat",
            "code-editor",
            "messenger",
            "mail",
            "notes",
        ] {
            assert!(ids.contains(&want), "{want}");
        }
    }
}
