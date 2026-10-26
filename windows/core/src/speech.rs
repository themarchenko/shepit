//! Speech result helpers. Mirrors `HallucinationFilter`, `WhisperKitEngine.retryLanguage` and
//! `LivePreview.join` in the Swift core.

// SpeechPresence.swift:34 (HallucinationFilter.wholeText)
const WHOLE_TEXT: [&str; 9] = [
    "дякую за перегляд",
    "дякую за увагу",
    "продовження слідує",
    "до зустрічі",
    "субтитри",
    "thank you for watching",
    "thanks for watching",
    "thank you",
    "bye",
];

// SpeechPresence.swift:38 (HallucinationFilter.fragments)
const FRAGMENTS: [&str; 6] = [
    "субтитри створив",
    "субтитри зробив",
    "dimatorzok",
    "amara.org",
    "subtitles by",
    "підпишіться на канал",
];

/// Swift `CharacterSet.letters`: general categories L* and M*. Nl (Roman numerals) and Nd are not letters.
fn is_swift_letter(c: char) -> bool {
    use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};
    matches!(
        c.general_category_group(),
        GeneralCategoryGroup::Letter | GeneralCategoryGroup::Mark
    )
}

/// SpeechPresence.swift:43 `HallucinationFilter.isJunk`.
///
/// `str::to_lowercase` and Swift `lowercased()` agree on Latin and Cyrillic; both are full Unicode
/// lowercase. Substring search is exact scalar matching in Rust; Swift's Foundation `contains` may
/// also match canonically equivalent sequences (precomposed vs decomposed), which can differ for
/// decomposed input.
pub fn is_junk(text: &str) -> bool {
    let lowered = text.to_lowercase();
    if FRAGMENTS.iter().any(|f| lowered.contains(f)) {
        return true;
    }
    let stripped: String = lowered
        .chars()
        .filter(|&c| is_swift_letter(c) || c == ' ')
        .collect();
    let normalized = stripped
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    WHOLE_TEXT.contains(&normalized.as_str())
}

/// Language to decode again with, or `None` when the first pass stands.
/// WhisperKitEngine.swift:57 `retryLanguage`.
pub fn retry_language(detected: Option<&str>, text: &str) -> Option<String> {
    let detected = detected?;
    if detected == "uk" || detected == "en" {
        return None;
    }
    // Cyrillic block U+0400..U+04FF, as in Swift.
    let cyrillic = text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c));
    Some(if cyrillic { "uk" } else { "en" }.to_string())
}

/// LivePreview.swift:127 `join`: non-empty parts separated by one space.
pub fn join(a: &str, b: &str) -> String {
    [a, b]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_ignores_case_punctuation_and_extra_spaces() {
        assert!(is_junk("  THANK   YOU, for watching!! "));
        assert!(is_junk("Дякую за увагу..."));
        assert!(!is_junk("thank you very much"));
        assert!(!is_junk(""));
    }

    #[test]
    fn stress_mark_does_not_break_whole_text_match() {
        assert!(is_junk("до зустрічі"));
        // Swift keeps the combining mark (category M), so the phrase no longer matches.
        assert!(!is_junk("до зустрічі\u{0301}"));
    }

    #[test]
    fn retry_picks_script() {
        assert_eq!(retry_language(Some("ru"), "привіт"), Some("uk".into()));
        assert_eq!(retry_language(Some("de"), "hello"), Some("en".into()));
        assert_eq!(retry_language(Some("uk"), "привіт"), None);
        assert_eq!(retry_language(None, "привіт"), None);
    }

    #[test]
    fn join_skips_empty() {
        assert_eq!(join("", ""), "");
        assert_eq!(join("a", "b"), "a b");
    }
}
