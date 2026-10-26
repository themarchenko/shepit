//! Text cleanup without a model. Mirrors `PassthroughFormatter` in the Swift core.

use unicode_segmentation::UnicodeSegmentation;

/// Non-empty runs of clusters that are not whitespace or newlines. Swift `isWhitespace` and `isNewline`
/// look at the first scalar of a grapheme cluster, so a space fused with a combining mark separates
/// words, and "\r\n" is one separator.
pub(crate) fn words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, g) in text.grapheme_indices(true) {
        let space = g.chars().next().is_some_and(char::is_whitespace);
        match (space, start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                out.push(&text[s..i]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
}

/// Formatter.swift `PassthroughFormatter.clean`: collapse whitespace runs to one space, trim, and
/// uppercase the first character.
///
/// Swift uppercases the whole first cluster, as here. `to_uppercase` expands like Swift (`ß` -> `SS`).
pub fn passthrough_clean(text: &str) -> String {
    let collapsed = words(text).join(" ");
    let mut clusters = collapsed.graphemes(true);
    match clusters.next() {
        None => String::new(),
        Some(first) => first.to_uppercase() + clusters.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_and_capitalizes() {
        assert_eq!(passthrough_clean("  hello \t\r\n world "), "Hello world");
        assert_eq!(passthrough_clean("   "), "");
        assert_eq!(passthrough_clean("ßa"), "SSa");
        assert_eq!(passthrough_clean("їжак"), "Їжак");
    }
}
