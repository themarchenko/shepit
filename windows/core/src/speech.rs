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

/// Minimum speech before the recognizer runs: DictationSession.swift:177 `speechSeconds >= 0.3`.
pub const MIN_SPEECH_SECS: f64 = 0.3;

/// SpeechPresence.swift:6 `speechSeconds(in:sampleRate:)`: seconds of 20 ms frames whose RMS is
/// above max(3 x the 20th-percentile frame, 0.006). The f32 sums run in the same order as Swift so
/// the threshold comparison matches bit for bit.
pub fn speech_seconds(samples: &[f32], sample_rate: u32) -> f64 {
    let frame = (sample_rate / 50) as usize; // 20 ms, SpeechPresence.swift:8
    if frame == 0 || samples.len() < frame {
        return 0.0;
    }
    let energies: Vec<f32> = samples
        .chunks_exact(frame)
        .map(|chunk| {
            let mut sum: f32 = 0.0;
            for &s in chunk {
                sum += s * s;
            }
            (sum / frame as f32).sqrt()
        })
        .collect();
    let mut sorted = energies.clone();
    sorted.sort_by(f32::total_cmp);
    // SpeechPresence.swift:21-22
    let noise_floor = sorted[sorted.len() / 5];
    let threshold = (noise_floor * 3.0).max(0.006);
    let speech_frames = energies.iter().filter(|&&e| e > threshold).count();
    speech_frames as f64 * frame as f64 / sample_rate as f64
}

/// DictationSession.swift:177 gate at the fixed 16 kHz the recorder delivers.
pub fn has_enough_speech(samples: &[f32]) -> bool {
    speech_seconds(samples, 16_000) >= MIN_SPEECH_SECS
}

/// DictationSession.swift:218 `String(text.prefix(60))` with newlines turned into spaces.
/// `prefix` counts Characters, so grapheme clusters here, not bytes or scalars.
pub fn done_preview(text: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    text.graphemes(true)
        .take(60)
        .collect::<String>()
        .replace('\n', " ")
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

    // Seeded LCG in place of Swift's Float.random (SpeechPresenceTests); no shared fixture because
    // the input is float audio (dictation.md:37).
    struct Lcg(u64);
    impl Lcg {
        fn next_unit(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        }
    }

    const FRAME: usize = 320;

    fn frames(zero: usize, level: f32, loud: usize) -> Vec<f32> {
        let mut v = vec![0.0; zero * FRAME];
        v.extend(std::iter::repeat_n(level, loud * FRAME));
        v
    }

    #[test]
    fn silence_has_no_speech() {
        assert_eq!(speech_seconds(&vec![0.0; 16_000], 16_000), 0.0);
    }

    #[test]
    fn a_burst_over_low_noise_is_about_half_a_second() {
        let mut rng = Lcg(42);
        let mut samples: Vec<f32> = (0..32_000).map(|_| rng.next_unit() * 0.002).collect();
        for (i, s) in samples.iter_mut().enumerate().take(16_000).skip(8_000) {
            *s += 0.3 * (i as f32 * 0.1).sin();
        }
        let secs = speech_seconds(&samples, 16_000);
        assert!((secs - 0.5).abs() < 0.05, "{secs}");
    }

    #[test]
    fn fewer_samples_than_one_frame_is_zero() {
        assert_eq!(speech_seconds(&vec![0.9; 319], 16_000), 0.0);
    }

    #[test]
    fn a_loud_trailing_partial_frame_is_ignored() {
        let mut samples = vec![0.0; 10 * FRAME];
        samples.extend(vec![0.9; 100]);
        assert_eq!(speech_seconds(&samples, 16_000), 0.0);
    }

    #[test]
    fn the_absolute_floor_is_0_006() {
        let above = speech_seconds(&frames(40, 0.0061, 10), 16_000);
        assert!((above - 0.2).abs() < 1e-9, "{above}");
        assert_eq!(speech_seconds(&frames(40, 0.0059, 10), 16_000), 0.0);
    }

    #[test]
    fn the_noise_floor_is_read_at_count_over_five() {
        // 10 frames: index 2 of the sorted energies is a loud one, so the threshold is 0.03.
        assert_eq!(speech_seconds(&frames(2, 0.01, 8), 16_000), 0.0);
    }

    #[test]
    fn fifteen_speech_frames_are_enough_fourteen_are_not() {
        assert!(has_enough_speech(&frames(35, 0.01, 15)));
        assert!(!has_enough_speech(&frames(36, 0.01, 14)));
    }

    #[test]
    fn done_preview_keeps_sixty_graphemes() {
        let long = "a".repeat(80);
        assert_eq!(done_preview(&long).chars().count(), 60);
        assert_eq!(done_preview("short"), "short");
    }

    #[test]
    fn done_preview_turns_newlines_into_spaces() {
        assert_eq!(done_preview("one\ntwo"), "one two");
    }

    #[test]
    fn done_preview_counts_cyrillic_per_character() {
        let text = "привіт".repeat(20);
        let out = done_preview(&text);
        assert_eq!(out.chars().count(), 60);
        assert!(text.starts_with(&out));
    }

    #[test]
    fn done_preview_does_not_split_a_grapheme_cluster() {
        // 59 letters then "e" + combining acute: one Character, so it stays whole at index 60.
        let text = format!("{}e\u{301}tail", "a".repeat(59));
        let out = done_preview(&text);
        assert!(out.ends_with("e\u{301}"));
        assert_eq!(out.chars().count(), 61);
    }
}
