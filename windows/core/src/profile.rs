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
/// Swift uses `URL(string:)?.host`. This is a small RFC 3986 authority parse, not a full
/// URL parser. Parity holds for absolute http(s) URLs and scheme-relative `//host/...`. Not
/// guaranteed: percent-encoded or IDN hosts (Swift may decode or reject), URLs Foundation rejects
/// for other malformed characters (only whitespace and control characters are rejected here), and
/// userinfo/port edge cases. Scheme-less input such as "claude.ai/new" has no host in both.
fn host_of(url: Option<&str>) -> Option<String> {
    let url = url?;
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let after_scheme = match url.find(':') {
        Some(i)
            if url[..i]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
                && i > 0 =>
        {
            &url[i + 1..]
        }
        _ => url,
    };
    let rest = after_scheme.strip_prefix("//")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let hostport = authority.rsplit('@').next().unwrap_or("");
    let host = if let Some(inner) = hostport.strip_prefix('[') {
        inner.split(']').next().unwrap_or("")
    } else {
        hostport.split(':').next().unwrap_or("")
    };
    if host.is_empty() {
        return None;
    }
    let host = host.to_lowercase();
    Some(
        host.strip_prefix("www.")
            .map(str::to_string)
            .unwrap_or(host),
    )
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

/// Profile.swift:150 `ModeResolver.wordCount`. Swift tests `Character.isWhitespace || isNewline`
/// per grapheme; Rust uses `char::is_whitespace`, which also covers LF, CR, NEL, LS and PS. They
/// differ only for whitespace fused with a combining mark.
pub fn word_count(text: &str) -> usize {
    text.split(char::is_whitespace)
        .filter(|w| !w.is_empty())
        .count()
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
}
