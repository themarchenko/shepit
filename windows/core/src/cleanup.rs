//! Text cleanup without a model. Mirrors `PassthroughFormatter` in the Swift core.

/// Formatter.swift `PassthroughFormatter.clean`: collapse whitespace runs to one space, trim, and
/// uppercase the first character.
///
/// Parity notes. Swift splits on `Character.isWhitespace || isNewline` per grapheme cluster; Rust
/// splits per `char` with `char::is_whitespace` (Unicode White_Space, which covers the Swift newline
/// set: LF, CR, NEL, LS, PS). They differ only for whitespace fused into a cluster with a combining
/// mark (Swift keeps it as one non-space character). Swift uppercases the whole first cluster;
/// Rust uppercases the first `char` only, so a first cluster whose combining marks have an uppercase
/// mapping differs. `to_uppercase` expands like Swift (`ß` -> `SS`).
pub fn passthrough_clean(text: &str) -> String {
    let collapsed = text
        .split(char::is_whitespace)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = collapsed.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
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
