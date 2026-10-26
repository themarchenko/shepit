//! Dictionary correction and learning. Mirrors `DictionaryCorrector` and `DictionaryLearning` in the Swift core
//! (apple/ShepitCore/Sources/ShepitCore/Learning/DictionaryCorrector.swift).
//!
//! Unicode parity with Swift, in one place. Swift iterates `Character` (extended grapheme clusters) and
//! asks the first scalar for its properties; this port iterates `char` (scalars).
//! - `Character.isLetter` is the first scalar's Alphabetic property, which is `char::is_alphabetic`.
//! - `Character.isNumber` is "has a Numeric_Type". `char::is_numeric` (Nd, Nl, No) matches except for a few
//!   CJK ideographs that are letters with a numeric value.
//! - `isUppercase` / `isLowercase` are the Uppercase / Lowercase properties, as `char::is_uppercase` /
//!   `char::is_lowercase`.
//! - `lowercased()` is `to_lowercase()`. Both apply full Unicode mappings; the final-sigma rule may differ.
//! - `trimmingCharacters(in: .whitespacesAndNewlines)` is `trim()` (White_Space). Same set in practice.
//!
//! Parity is exact on ASCII, precomposed Cyrillic and Latin text, which is all the fixtures use. It is not
//! guaranteed for decomposed text (a base letter plus a combining mark is one Swift `Character` but two
//! `char`s here), emoji sequences, or the exotic cases above.

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
        if spoken.is_empty() || key.chars().count() < 5 {
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
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
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
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_numeric())
        .collect()
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
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
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &(byte, c)) in chars.iter().enumerate() {
        // An apostrophe inside a word belongs to it ("м'ята", "don't").
        let inner = matches!(c, '\'' | '’' | 'ʼ')
            && start.is_some()
            && chars.get(i + 1).is_some_and(|&(_, n)| n.is_alphabetic());
        if c.is_alphabetic() || c.is_numeric() || inner {
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

fn spoken_parts(term: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut previous: Option<char> = None;
    for c in term.chars() {
        if !(c.is_alphabetic() || c.is_numeric()) {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else {
            if previous.is_some_and(|p| p.is_lowercase()) && c.is_uppercase() && !current.is_empty()
            {
                parts.push(std::mem::take(&mut current));
            }
            current.push(c);
        }
        previous = Some(c);
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
    let mut key = String::new();
    for c in s.chars() {
        let mapped = match c {
            'a' | 'e' | 'i' | 'o' | 'u' | 'y' => Some('A'),
            'b' | 'p' => Some('P'),
            'f' | 'v' | 'w' | 'F' => Some('F'),
            'g' | 'h' | 'k' | 'c' | 'q' | 'K' => Some('K'),
            'd' | 't' | 'T' => Some('T'),
            's' | 'z' | 'S' => Some('S'),
            'j' | 'J' => Some('J'),
            'l' | 'm' | 'n' | 'r' => Some(c.to_ascii_uppercase()),
            _ if c.is_numeric() => Some(c),
            _ => None,
        };
        let Some(mapped) = mapped else { continue };
        if key.ends_with(mapped) {
            continue;
        }
        key.push(mapped);
    }
    key
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

fn cyrillic(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "h",
        'ґ' => "g",
        'д' => "d",
        'е' => "e",
        'є' => "ye",
        'ж' => "zh",
        'з' => "z",
        'и' => "y",
        'і' => "i",
        'ї' => "yi",
        'й' => "y",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "kh",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "shch",
        'ь' => "",
        'ю' => "yu",
        'я' => "ya",
        'ы' => "y",
        'э' => "e",
        'ё' => "yo",
        'ъ' => "",
        '\'' | '’' | 'ʼ' => "",
        _ => return None,
    })
}

fn transliterate(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match cyrillic(c) {
            Some(mapped) => out.push_str(mapped),
            None => out.push(c),
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
        // Swift `term.count` counts graphemes. Terms with any non-ASCII scalar are rejected below, so the
        // only cluster that matters here is "\r\n", which Swift counts as one.
        let count = term.chars().count() - term.matches("\r\n").count();
        if !(2..=40).contains(&count) || known.contains(&term.to_lowercase()) {
            continue;
        }
        if !term.is_ascii() || !term.chars().any(|c| c.is_alphabetic()) {
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
}
