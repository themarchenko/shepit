//! The system prompt and the per-dictation message. Mirrors `PromptBuilder.swift`.

use crate::profile::{Mode, Profile};

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

/// `PromptBuilder.systemPrompt` over every built-in profile. `platform` replaces the one word
/// "macOS" in the first sentence: with "macOS" the output plus a final newline is the golden
/// `shared/prompts/system-prompt.md`; the Windows app passes "Windows". Notes are trimmed, and
/// blank notes leave out the Speaker section.
pub fn system_prompt(platform: &str, speaker_notes: Option<&str>) -> String {
    let _ = (platform, speaker_notes);
    todo!("task 9 chunk A: system prompt")
}

/// `PromptBuilder.userMessage`. The context object is built by a dedicated escaper that matches
/// Foundation's `JSONSerialization` with `.sortedKeys` (it writes `/` as `\/`), not by serde_json.
pub fn user_message(request: &FormatRequest) -> String {
    let _ = request;
    todo!("task 9 chunk A: user message")
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
