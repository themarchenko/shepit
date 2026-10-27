//! The system prompt and the per-dictation message. Mirrors `PromptBuilder.swift`.

use crate::profile::{all_profiles, Mode, Profile};
use unicode_segmentation::UnicodeSegmentation;

/// `SpeakerNotes.defaultText` (AppSettings.swift:248-250).
pub const DEFAULT_SPEAKER_NOTES: &str = "Native Ukrainian speaker who also dictates in English and mixes English technical terms into Ukrainian sentences.";

/// The context keys of the message. `url` is never captured on either platform, but the key exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptContext {
    pub app: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub text_before_caret: Option<String>,
    pub selected_text: Option<String>,
}

/// Mirrors `FormatRequest` (Formatter.swift:3-22); `secure` stands for `context.secureInput`.
#[derive(Debug, Clone)]
pub struct FormatRequest {
    pub transcript: String,
    pub language_hint: Option<String>,
    pub mode: Mode,
    pub profile: &'static Profile,
    pub context: PromptContext,
    pub dictionary: Vec<String>,
    /// (raw transcript, accepted final text) pairs.
    pub examples: Vec<(String, String)>,
    pub secure: bool,
}

/// Text before the profile list. `{platform}` is the one word that differs per OS.
const PROMPT_HEAD: &str = r#"You are the text formatter inside Shepit, a personal dictation app on {platform}. The user speaks Ukrainian and English, often mixed in one sentence. You receive the raw speech-to-text transcript plus context about where the text will be inserted, and you return the text that should be inserted. You never answer the content, never add facts or requirements, never translate unless the transcript itself is an explicit instruction to translate.

## Cleanup rules (all modes except raw)

- Remove filler words and false starts ("ем", "ну", "типу", "um", "uh", "like" as filler, repeated words).
- Apply self-corrections: "на п'яту, ні, краще на шосту" becomes "на шосту"; "Monday, actually no, Wednesday" becomes "Wednesday".
- Punctuate and capitalize. Split into paragraphs at topic changes. A spoken enumeration becomes a list only when it has three or more items and the profile allows lists.
- Keep the wording and register of the speaker. Do not paraphrase for style.
- Output language is the spoken language. Inside Ukrainian text, English product names, identifiers, file paths, commands and flags stay in Latin script with canonical spelling (handleSubmit, UserService.swift, git rebase, npm run build, Vercel, Supabase, Claude Code). Fix transliterations the recognizer produced ("хендл сабміт" is handleSubmit) using the dictionary and the on-screen context.
- Anglicisms: the speaker says English technical terms inside Ukrainian speech and the recognizer writes them in Cyrillic ("пул реквест", "рефакторінг", "комміт", "деплой"). Write such terms in Latin with standard English spelling when they name a technical concept, action, tool or artifact (pull request, refactoring, commit, deploy, staging, production, feature flag, prompt, merge). Ukrainian-inflected forms used as ordinary words stay in Cyrillic as spoken (закомітити, задеплоїти, фіча, таска).
- Numbers: digits for quantities, dates and money; words where the speaker clearly means a word.
- Ukrainian typography: apostrophe ' and quotes «». Dashes are plain hyphens.
- If the transcript is an instruction about the text itself ("напиши англійською", "make it a list"), follow it.

## Context

"text_before_caret" is what the field already holds before the cursor. Use it for spelling and to continue a sentence (lowercase start, no repeated words). Never copy it into your output: the text you return is inserted at the cursor, after it, so anything taken from it appears twice. Only the transcript becomes output.

## Modes

- clean: cleanup only.
- prompt: cleanup, then shape the text as a prompt for an AI assistant: one goal sentence first, then context, then constraints or questions as separate short paragraphs or a "-" list. Only reorganize what was said. Never invent a requirement that was not spoken. Short transcripts stay short.
- raw: transcript with punctuation only.

## Profiles

The context names the profile of the target app. Apply its rules.
"#;

/// The Output section, after the optional Speaker section.
const PROMPT_TAIL: &str = r#"
## Output

Return only a JSON object: {"text": string, "language": "uk" | "en" | "mixed", "new_terms": string[]}. "new_terms" lists proper nouns, product names or identifiers whose spelling you fixed and that are not in the dictionary; never include ordinary words or commands. No commentary outside the JSON."#;

/// `PromptBuilder.systemPrompt` over every built-in profile. `platform` replaces the one word
/// "macOS" in the first sentence: with "macOS" the output plus a final newline is the golden
/// `shared/prompts/system-prompt.md`; the Windows app passes "Windows". Notes are trimmed, and
/// blank notes leave out the Speaker section.
pub fn system_prompt(platform: &str, speaker_notes: Option<&str>) -> String {
    let mut s = PROMPT_HEAD.replacen("{platform}", platform, 1);
    for p in all_profiles() {
        s.push_str(&format!("\n### {} ({})\n", p.id, p.name));
        s.push_str(&format!(
            "- Markdown: {}.\n",
            if p.allows_markdown {
                "allowed"
            } else {
                "not allowed"
            }
        ));
        for rule in p.style_rules {
            s.push_str(&format!("- {rule}\n"));
        }
    }
    // Swift trims whitespace and newlines (trimmingCharacters(in: .whitespacesAndNewlines)).
    if let Some(notes) = speaker_notes.map(str::trim).filter(|n| !n.is_empty()) {
        s.push_str(&format!(
            "\n## Speaker\n\nUse this to tell recognizer errors from what was meant:\n{notes}\n"
        ));
    }
    s.push_str(PROMPT_TAIL);
    s
}

/// Writes `s` as a JSON string the way Foundation's `JSONSerialization` does: `/` becomes `\/`,
/// control characters below U+0020 become `\n`, `\r`, `\t`, `\b`, `\f` or `\u00xx`, and every
/// other character (U+2028 and non-ASCII included) stays as is.
fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// First `n` grapheme clusters (Swift `String.prefix`).
fn prefix_graphemes(s: &str, n: usize) -> &str {
    match s.grapheme_indices(true).nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Last `n` grapheme clusters (Swift `String.suffix`).
fn suffix_graphemes(s: &str, n: usize) -> &str {
    let total = s.graphemes(true).count();
    if total <= n {
        return s;
    }
    match s.grapheme_indices(true).nth(total - n) {
        Some((i, _)) => &s[i..],
        None => s,
    }
}

/// `PromptBuilder.userMessage`. The context object is built by a dedicated escaper that matches
/// Foundation's `JSONSerialization` with `.sortedKeys` (it writes `/` as `\/`), not by serde_json.
pub fn user_message(request: &FormatRequest) -> String {
    let r = request;
    let mut fields: Vec<(&str, String)> = vec![
        ("profile", r.profile.id.to_string()),
        ("mode", r.mode.as_str().to_string()),
        (
            "language_hint",
            r.language_hint
                .clone()
                .unwrap_or_else(|| "auto".to_string()),
        ),
    ];
    let c = &r.context;
    if let Some(v) = &c.app {
        fields.push(("app", v.clone()));
    }
    if let Some(v) = &c.bundle_id {
        fields.push(("bundle_id", v.clone()));
    }
    if let Some(v) = &c.window_title {
        fields.push(("window_title", prefix_graphemes(v, 200).to_string()));
    }
    if let Some(v) = &c.url {
        fields.push(("url", v.clone()));
    }
    if let Some(v) = c.text_before_caret.as_ref().filter(|v| !v.is_empty()) {
        fields.push(("text_before_caret", suffix_graphemes(v, 500).to_string()));
    }
    if let Some(v) = c.selected_text.as_ref().filter(|v| !v.is_empty()) {
        fields.push(("selected_text", prefix_graphemes(v, 2000).to_string()));
    }
    fields.sort_by(|a, b| a.0.cmp(b.0));

    let mut json = String::from("{");
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        push_json_string(&mut json, k);
        json.push(':');
        push_json_string(&mut json, v);
    }
    json.push('}');

    let mut m = format!("Context:\n{json}\n");
    // Newest entries: learned terms are appended, so a long dictionary would otherwise never show
    // its recent ones to the agent.
    if !r.dictionary.is_empty() {
        let start = r.dictionary.len().saturating_sub(120);
        m.push_str("\nDictionary (preferred spellings): ");
        m.push_str(&r.dictionary[start..].join(", "));
        m.push('\n');
    }
    if !r.examples.is_empty() {
        m.push_str("\nRecent accepted examples for this profile:\n");
        for (raw, fin) in r.examples.iter().take(5) {
            m.push_str(&format!("Transcript: {raw}\nFinal: {fin}\n\n"));
        }
    }
    m.push_str(&format!("\nTranscript:\n{}", r.transcript));
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{all_profiles, profile_by_id};

    fn golden() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../shared/prompts/system-prompt.md");
        std::fs::read_to_string(path).expect("golden prompt")
    }

    fn request(transcript: &str, mode: Mode, profile: &str) -> FormatRequest {
        FormatRequest {
            transcript: transcript.to_string(),
            language_hint: None,
            mode,
            profile: profile_by_id(profile).unwrap(),
            context: PromptContext::default(),
            dictionary: Vec::new(),
            examples: Vec::new(),
            secure: false,
        }
    }

    #[test]
    fn the_macos_prompt_equals_the_golden_byte_for_byte() {
        let actual = system_prompt("macOS", Some(DEFAULT_SPEAKER_NOTES)) + "\n";
        assert_eq!(actual, golden());
    }

    #[test]
    fn the_windows_prompt_differs_from_the_golden_in_one_word_only() {
        let actual = system_prompt("Windows", Some(DEFAULT_SPEAKER_NOTES)) + "\n";
        let want = golden().replacen("dictation app on macOS.", "dictation app on Windows.", 1);
        assert_ne!(want, golden());
        assert_eq!(actual, want);
        assert!(!actual.contains("macOS"));
    }

    #[test]
    fn notes_are_trimmed() {
        let padded = system_prompt("macOS", Some("  \n Speaks Ukrainian.\t\n"));
        let plain = system_prompt("macOS", Some("Speaks Ukrainian."));
        assert_eq!(padded, plain);
        assert!(plain.contains("\n## Speaker\n\nUse this to tell recognizer errors from what was meant:\nSpeaks Ukrainian.\n"));
    }

    #[test]
    fn blank_or_missing_notes_give_no_speaker_section() {
        for notes in [None, Some(""), Some("   \n\t ")] {
            let prompt = system_prompt("macOS", notes);
            assert!(!prompt.contains("## Speaker"), "{notes:?}");
            assert!(prompt.contains("## Output"), "{notes:?}");
        }
    }

    // Ports of FormattingTests.swift:29-58.

    #[test]
    fn the_system_prompt_lists_every_profile() {
        let prompt = system_prompt("macOS", None);
        for p in all_profiles() {
            assert!(
                prompt.contains(&format!("### {}", p.id)),
                "missing {}",
                p.id
            );
        }
        assert!(prompt.contains("\"new_terms\""));
    }

    #[test]
    fn the_user_message_carries_context_and_transcript() {
        let mut r = request("зроби рефакторинг", Mode::Prompt, "claude-code-terminal");
        r.language_hint = Some("uk".into());
        r.context.bundle_id = Some("com.apple.Terminal".into());
        r.context.app = Some("Terminal".into());
        r.context.window_title = Some("Claude Code".into());
        r.dictionary = vec!["Shepit".into()];
        let m = user_message(&r);
        assert!(m.contains("\"profile\":\"claude-code-terminal\""), "{m}");
        assert!(m.contains("\"mode\":\"prompt\""), "{m}");
        assert!(
            m.contains("Dictionary (preferred spellings): Shepit"),
            "{m}"
        );
        assert!(m.ends_with("Transcript:\nзроби рефакторинг"), "{m}");
    }

    #[test]
    fn the_message_starts_with_a_compact_context_object_with_sorted_keys() {
        let m = user_message(&request("hello", Mode::Clean, "generic"));
        assert_eq!(
            m,
            "Context:\n{\"language_hint\":\"auto\",\"mode\":\"clean\",\"profile\":\"generic\"}\n\nTranscript:\nhello"
        );
    }

    #[test]
    fn the_dictionary_keeps_the_newest_120_entries() {
        let mut r = request("x", Mode::Clean, "generic");
        r.language_hint = Some("en".into());
        r.dictionary = (1..=130).map(|i| format!("term{i}")).collect();
        let m = user_message(&r);
        let line = m
            .lines()
            .find(|l| l.starts_with("Dictionary (preferred spellings): "))
            .expect("dictionary line");
        let entries: Vec<&str> = line["Dictionary (preferred spellings): ".len()..]
            .split(", ")
            .collect();
        assert_eq!(entries.len(), 120);
        assert_eq!(entries.first(), Some(&"term11"));
        assert_eq!(entries.last(), Some(&"term130"));
    }

    #[test]
    fn a_title_with_slashes_quotes_and_control_characters_is_escaped_like_foundation() {
        let mut r = request("x", Mode::Clean, "generic");
        r.context.window_title = Some("a/b \"q\" c\\d\n\t\u{1}".into());
        let m = user_message(&r);
        assert!(
            m.contains(r#""window_title":"a\/b \"q\" c\\d\n\t\u0001""#),
            "{m}"
        );
    }
}
