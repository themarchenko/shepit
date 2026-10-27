//! Dictionary correction and learning. Mirrors `DictionaryCorrector` and `DictionaryLearning` in the Swift core
//! (apple/ShepitCore/Sources/ShepitCore/Learning/DictionaryCorrector.swift).
//!
//! Unicode parity with Swift, in one place. Swift iterates `Character` (extended grapheme clusters) and
//! asks the first scalar for its properties; this port iterates clusters from `unicode-segmentation` and
//! does the same.
//! - `Character.isLetter` is the first scalar's Alphabetic property, which is `char::is_alphabetic`.
//! - `Character.isNumber` is "has a Numeric_Type". `char::is_numeric` (Nd, Nl, No) matches except for a few
//!   CJK ideographs that are letters with a numeric value.
//! - `isUppercase` / `isLowercase` hold when the cluster is unchanged by `uppercased()` / `lowercased()`
//!   and its first scalar is cased (probed on Swift: "A" yes, "1" no, U+02B0 both, U+01C5 neither).
//! - `lowercased()` is `to_lowercase()`. Both apply full Unicode mappings; the final-sigma rule may differ.
//! - `trimmingCharacters(in: .whitespacesAndNewlines)` is `trim()` (White_Space). Same set in practice.
//! - Swift compares `Character` and `String` by canonical equivalence; this port compares bytes. The one
//!   place it matters here is the transliteration table, which lists the decomposed forms of its three
//!   precomposed letters (й, ё, ї). Other decomposed text only differs for exotic input.

use unicode_segmentation::UnicodeSegmentation;

use std::collections::HashSet;

/// Most dictionary terms kept (Swift `DictionaryLearning.maxTerms`).
pub const MAX_TERMS: usize = 300;

struct Term {
    text: String,
    key: String,
    letters: String,
    /// Spoken words: "handleSubmit" and "UserService.swift" count their camel-case and dotted parts.
    parts: usize,
}

impl Term {
    fn new(raw: &str) -> Option<Term> {
        let text = raw.trim().to_string();
        let spoken = spoken_parts(&text);
        let key: String = spoken.iter().map(|p| sound_key(p)).collect();
        if spoken.is_empty() || key.graphemes(true).count() < 5 {
            return None;
        }
        Some(Term {
            letters: letters(&text),
            parts: spoken.len(),
            text,
            key,
        })
    }
}

struct Match {
    first: usize,
    last: usize,
    term: String,
    score: f64,
}

struct Token {
    text: String,
    start: usize,
    end: usize,
}

impl Token {
    fn is_latin(&self) -> bool {
        self.text.is_ascii()
    }
}

/// Replaces words that sound like a dictionary entry with that entry.
pub fn correct(text: &str, dictionary: &[String]) -> String {
    let tokens = tokenize(text);
    if tokens.is_empty() || dictionary.is_empty() {
        return text.to_string();
    }
    let terms: Vec<Term> = dictionary.iter().filter_map(|d| Term::new(d)).collect();
    if terms.is_empty() {
        return text.to_string();
    }

    let keys: Vec<String> = tokens.iter().map(|t| sound_key(&t.text)).collect();
    // (first, last, index into terms), in the same order as Swift: term, then size, then start.
    let mut windows: Vec<(usize, usize, usize)> = Vec::new();
    for (ti, term) in terms.iter().enumerate() {
        for size in 1..=tokens.len().min(term.parts) {
            for start in 0..=(tokens.len() - size) {
                windows.push((start, start + size - 1, ti));
            }
        }
    }

    // Tokens that already spell a term ("handleSubmit", "UserService" + "swift") are kept as written.
    let mut protected = vec![false; tokens.len()];
    for &(first, last, ti) in &windows {
        let joined: String = tokens[first..=last]
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        if letters(&joined) == terms[ti].letters {
            protected[first..=last].iter_mut().for_each(|p| *p = true);
        }
    }

    let mut candidates: Vec<Match> = Vec::new();
    for &(first, last, ti) in &windows {
        if protected[first..=last].iter().any(|&p| p) {
            continue;
        }
        let term = &terms[ti];
        if term.parts < 2 && tokens[first..=last].iter().all(Token::is_latin) {
            continue;
        }
        let window_key: String = keys[first..=last].concat();
        let Some(score) = similarity(&window_key, &term.key) else {
            continue;
        };
        candidates.push(Match {
            first,
            last,
            term: term.text.clone(),
            score,
        });
    }
    if candidates.is_empty() {
        return text.to_string();
    }

    // Best score first, then the longer window, so "клод код" wins over "клод" alone. Swift sorts by the
    // tuple (score, length) descending; the stable sort keeps generation order on exact ties, as Swift's does.
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then((b.last - b.first).cmp(&(a.last - a.first)))
    });
    let mut taken = vec![false; tokens.len()];
    let mut chosen: Vec<&Match> = Vec::new();
    for m in &candidates {
        if taken[m.first..=m.last].iter().any(|&t| t) {
            continue;
        }
        taken[m.first..=m.last].iter_mut().for_each(|t| *t = true);
        chosen.push(m);
    }

    // Replace from the end so earlier byte ranges stay valid.
    let mut result = text.to_string();
    chosen.sort_by_key(|m| std::cmp::Reverse(m.first));
    for m in chosen {
        result.replace_range(tokens[m.first].start..tokens[m.last].end, &m.term);
    }
    result
}

/// None when the keys are too far apart to be the same term.
fn similarity(a: &str, b: &str) -> Option<f64> {
    let a: Vec<&str> = a.graphemes(true).collect();
    let b: Vec<&str> = b.graphemes(true).collect();
    if a.len() < 5 || b.len() < 5 {
        return None;
    }
    if a == b {
        return Some(1.0);
    }
    // Swift: min(a.count, b.count) >= 7 and levenshtein <= 1. Shorter keys must match exactly.
    if a.len().min(b.len()) < 7 || levenshtein(&a, &b) > 1 {
        return None;
    }
    Some(1.0 - 1.0 / a.len().max(b.len()) as f64)
}

/// Lowercased letters and digits only.
fn letters(s: &str) -> String {
    s.to_lowercase()
        .graphemes(true)
        .filter(|g| is_letter(g) || is_number(g))
        .collect()
}

fn levenshtein(a: &[&str], b: &[&str]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = previous.clone();
    for i in 1..=a.len() {
        current[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            current[j] = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

fn tokenize(text: &str) -> Vec<Token> {
    let clusters: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &(byte, g)) in clusters.iter().enumerate() {
        // An apostrophe inside a word belongs to it ("м'ята", "don't").
        let inner = matches!(g, "'" | "’" | "ʼ")
            && start.is_some()
            && clusters.get(i + 1).is_some_and(|&(_, n)| is_letter(n));
        if is_letter(g) || is_number(g) || inner {
            if start.is_none() {
                start = Some(byte);
            }
        } else if let Some(s) = start.take() {
            tokens.push(Token {
                text: text[s..byte].to_string(),
                start: s,
                end: byte,
            });
        }
    }
    if let Some(s) = start {
        tokens.push(Token {
            text: text[s..].to_string(),
            start: s,
            end: text.len(),
        });
    }
    tokens
}

fn first_scalar(g: &str) -> Option<char> {
    g.chars().next()
}

fn is_letter(g: &str) -> bool {
    first_scalar(g).is_some_and(char::is_alphabetic)
}

fn is_number(g: &str) -> bool {
    first_scalar(g).is_some_and(char::is_numeric)
}

fn is_cased(g: &str) -> bool {
    first_scalar(g).is_some_and(|c| c.is_uppercase() || c.is_lowercase())
}

fn is_uppercase(g: &str) -> bool {
    is_cased(g) && g.to_uppercase() == g
}

fn is_lowercase(g: &str) -> bool {
    is_cased(g) && g.to_lowercase() == g
}

fn spoken_parts(term: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut previous: Option<&str> = None;
    for g in term.graphemes(true) {
        if !(is_letter(g) || is_number(g)) {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else {
            if previous.is_some_and(is_lowercase) && is_uppercase(g) && !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            current.push_str(g);
        }
        previous = Some(g);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// A coarse pronunciation key: consonants in loose classes, every vowel run as "A", repeats collapsed.
/// English spelling quirks that matter for terms spoken by Ukrainians are handled first (silent final
/// "e", "qu" said as "кв", soft "c").
fn sound_key(word: &str) -> String {
    let mut s = transliterate(&word.to_lowercase());
    if word.is_ascii() {
        s = drop_silent_e(&s);
    }
    const DIGRAPHS: [(&str, &str); 17] = [
        ("shch", "J"),
        ("sch", "J"),
        ("tch", "J"),
        ("dzh", "J"),
        ("sh", "J"),
        ("ch", "J"),
        ("zh", "J"),
        ("ph", "F"),
        ("th", "T"),
        ("kh", "K"),
        ("ck", "K"),
        ("qu", "KF"),
        ("ts", "S"),
        ("x", "KS"),
        ("ce", "Se"),
        ("ci", "Si"),
        ("cy", "Sy"),
    ];
    // Swift `replacingOccurrences` replaces left to right without overlap, like `str::replace`.
    for (from, to) in DIGRAPHS {
        s = s.replace(from, to);
    }
    let mut key: Vec<String> = Vec::new();
    for g in s.graphemes(true) {
        let mapped = match g {
            "a" | "e" | "i" | "o" | "u" | "y" => Some("A".to_string()),
            "b" | "p" => Some("P".to_string()),
            "f" | "v" | "w" | "F" => Some("F".to_string()),
            "g" | "h" | "k" | "c" | "q" | "K" => Some("K".to_string()),
            "d" | "t" | "T" => Some("T".to_string()),
            "s" | "z" | "S" => Some("S".to_string()),
            "j" | "J" => Some("J".to_string()),
            "l" | "m" | "n" | "r" => Some(g.to_ascii_uppercase()),
            _ if is_number(g) => Some(g.to_string()),
            _ => None,
        };
        let Some(mapped) = mapped else { continue };
        if key.last() == Some(&mapped) {
            continue;
        }
        key.push(mapped);
    }
    key.concat()
}

fn drop_silent_e(s: &str) -> String {
    // "code", "claude", "supabase", "handle": the final "e" is not spoken.
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 3 || chars[chars.len() - 1] != 'e' {
        return s.to_string();
    }
    if "aeiouy".contains(chars[chars.len() - 2]) {
        s.to_string()
    } else {
        chars[..chars.len() - 1].iter().collect()
    }
}

fn cyrillic(g: &str) -> Option<&'static str> {
    Some(match g {
        "а" => "a",
        "б" => "b",
        "в" => "v",
        "г" => "h",
        "ґ" => "g",
        "д" => "d",
        "е" => "e",
        "є" => "ye",
        "ж" => "zh",
        "з" => "z",
        "и" => "y",
        "і" => "i",
        "ї" | "і\u{308}" => "yi",
        "й" | "и\u{306}" => "y",
        "к" => "k",
        "л" => "l",
        "м" => "m",
        "н" => "n",
        "о" => "o",
        "п" => "p",
        "р" => "r",
        "с" => "s",
        "т" => "t",
        "у" => "u",
        "ф" => "f",
        "х" => "kh",
        "ц" => "ts",
        "ч" => "ch",
        "ш" => "sh",
        "щ" => "shch",
        "ь" => "",
        "ю" => "yu",
        "я" => "ya",
        "ы" => "y",
        "э" => "e",
        "ё" | "е\u{308}" => "yo",
        "ъ" => "",
        "'" | "’" | "ʼ" => "",
        _ => return None,
    })
}

fn transliterate(s: &str) -> String {
    let mut out = String::new();
    for g in s.graphemes(true) {
        match cyrillic(g) {
            Some(mapped) => out.push_str(mapped),
            None => out.push_str(g),
        }
    }
    out
}

/// Reported terms that are not yet in `existing`, in report order.
///
/// Picks dictionary candidates out of the terms Claude reports as fixed. Only proper-noun-like tokens are
/// kept: a capital letter, a dot, an underscore or a digit inside, Latin script, at most three words.
pub fn new_entries(reported: &[String], existing: &[String]) -> Vec<String> {
    let mut known: HashSet<String> = existing.iter().map(|e| e.to_lowercase()).collect();
    let mut out: Vec<String> = Vec::new();
    for raw in reported {
        let term = raw.trim();
        let count = term.graphemes(true).count();
        if !(2..=40).contains(&count) || known.contains(&term.to_lowercase()) {
            continue;
        }
        if !term.is_ascii() || !term.chars().any(|c| c.is_alphabetic()) {
            continue;
        }
        // A newline or NUL inside a term would go back to the agent as part of the dictionary prompt.
        if term.chars().any(|c| c.is_control()) {
            continue;
        }
        // `split(separator: " ")` drops empty pieces, so filter them.
        if term.split(' ').filter(|p| !p.is_empty()).count() > 3 {
            continue;
        }
        let marked = term
            .chars()
            .any(|c| c.is_uppercase() || c.is_numeric() || c == '.' || c == '_');
        if !marked {
            continue;
        }
        known.insert(term.to_lowercase());
        out.push(term.to_string());
    }
    out.truncate(MAX_TERMS.saturating_sub(existing.len()));
    out
}

/// The terms of a hand-edited dictionary (the Dictionary pane, SettingsView.swift:197-201): every
/// item is split on `\r\n`, `\r` or `\n`, trimmed, and empty pieces are dropped. Order and duplicates
/// are kept and there is no cap, as on macOS.
pub fn normalize_terms(items: &[String]) -> Vec<String> {
    items
        .iter()
        .flat_map(|item| item.split(['\r', '\n']))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of Swift `testSoundKeysAgreeAcrossScripts`.
    #[test]
    fn sound_keys_agree_across_scripts() {
        assert_eq!(sound_key("клод"), sound_key("Claude"));
        assert_eq!(sound_key("реквест"), sound_key("request"));
        assert_eq!(sound_key("супабейс"), sound_key("Supabase"));
        assert_eq!(
            spoken_parts("UserService.swift"),
            vec!["User", "Service", "swift"]
        );
    }

    // The fixtures never reach the one-edit branch, so pin the thresholds here.
    #[test]
    fn similarity_thresholds() {
        assert_eq!(similarity("KLATKAT", "KLATKAT"), Some(1.0));
        assert_eq!(similarity("KLATKAT", "KLATKAP"), Some(1.0 - 1.0 / 7.0));
        assert_eq!(similarity("KLATKA", "KLATKP"), None);
        assert_eq!(similarity("KLATKAT", "KLATKPP"), None);
        assert_eq!(similarity("KLAT", "KLAT"), None);
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn terms_are_split_on_every_line_ending() {
        let got = normalize_terms(&strings(&["A\r\nB", "C\rD", "E\nF"]));
        assert_eq!(got, strings(&["A", "B", "C", "D", "E", "F"]));
    }

    #[test]
    fn terms_are_trimmed_and_blank_lines_dropped() {
        let got = normalize_terms(&strings(&["  Vercel  ", "", "   ", "\t", "x\n\n\ny", "\n"]));
        assert_eq!(got, strings(&["Vercel", "x", "y"]));
    }

    #[test]
    fn terms_keep_order_and_duplicates() {
        let got = normalize_terms(&strings(&["B", "a", "B", "A"]));
        assert_eq!(got, strings(&["B", "a", "B", "A"]));
    }

    #[test]
    fn hand_entered_terms_are_not_capped() {
        let many: Vec<String> = (0..350).map(|i| format!("Term{i}")).collect();
        assert_eq!(normalize_terms(&many).len(), 350);
    }

    #[test]
    fn learning_fills_only_the_room_left_under_the_cap() {
        let reported = strings(&["Supabase", "Vercel"]);
        let existing = |n: usize| -> Vec<String> { (0..n).map(|i| format!("Old{i}")).collect() };
        assert_eq!(
            new_entries(&reported, &existing(299)),
            strings(&["Supabase"])
        );
        assert!(new_entries(&reported, &existing(300)).is_empty());
        assert!(new_entries(&reported, &existing(350)).is_empty());
    }

    #[test]
    fn learning_looks_at_the_list_as_it_is_now() {
        // A term typed by hand since the last dictation is not learned a second time.
        let reported = strings(&["Supabase", "handleSubmit"]);
        assert_eq!(
            new_entries(&reported, &strings(&["supabase"])),
            strings(&["handleSubmit"])
        );
    }
}
