//! Reading the agent's reply and cleaning the text. Mirrors `JSONExtraction.firstObject`,
//! `PromptBuilder.parseReply` and `CodexFormatter.finish` (the Claude copy is the same).

use crate::profile::Profile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedReply {
    pub text: String,
    pub language: Option<String>,
    pub new_terms: Vec<String>,
}

/// The first JSON object of a reply that may sit in prose or code fences, with the raw
/// newline and tab retry. `None` when there is no object or no string `text`. A `new_terms`
/// that is not an array of strings counts as empty.
pub fn parse_reply(reply: &str) -> Option<ParsedReply> {
    let _ = reply;
    todo!("task 9 chunk A: parse_reply")
}

/// Trim newlines at both ends, strip trailing newlines again when the profile asks, uppercase a
/// lowercase first letter.
pub fn finish(text: &str, profile: &Profile) -> String {
    let _ = (text, profile);
    todo!("task 9 chunk A: finish")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::profile_by_id;

    fn generic() -> &'static Profile {
        profile_by_id("generic").unwrap()
    }

    // Ports of FormattingTests.swift:5-25 and 60-65.

    #[test]
    fn a_plain_object_parses() {
        let p = parse_reply(r#"{"text": "Hi", "language": "en", "new_terms": []}"#).unwrap();
        assert_eq!(p.text, "Hi");
        assert_eq!(p.language.as_deref(), Some("en"));
        assert!(p.new_terms.is_empty());
    }

    #[test]
    fn prose_and_a_fence_around_the_object_are_ignored() {
        let reply = "Sure, here it is:\n```json\n{\"text\": \"Привіт\", \"language\": \"uk\", \"new_terms\": [\"Vercel\"]}\n```";
        let p = parse_reply(reply).unwrap();
        assert_eq!(p.text, "Привіт");
        assert_eq!(p.new_terms, ["Vercel"]);
    }

    #[test]
    fn a_raw_newline_inside_a_string_is_repaired() {
        let reply = "{\"text\": \"line one\nline two\", \"language\": \"en\", \"new_terms\": []}";
        assert_eq!(parse_reply(reply).unwrap().text, "line one\nline two");
    }

    #[test]
    fn a_raw_tab_inside_a_string_is_repaired() {
        let reply = "{\"text\": \"a\tb\", \"language\": \"en\", \"new_terms\": []}";
        assert_eq!(parse_reply(reply).unwrap().text, "a\tb");
    }

    #[test]
    fn a_reply_without_an_object_is_none() {
        assert_eq!(parse_reply("just text"), None);
        assert_eq!(parse_reply(""), None);
    }

    #[test]
    fn a_full_reply_gives_text_language_and_terms() {
        let p = parse_reply(r#"{"text":"Hello","language":"en","new_terms":["X"]}"#).unwrap();
        assert_eq!(
            p,
            ParsedReply {
                text: "Hello".into(),
                language: Some("en".into()),
                new_terms: vec!["X".into()],
            }
        );
    }

    #[test]
    fn a_missing_or_non_string_text_is_none() {
        assert_eq!(parse_reply(r#"{"language":"en"}"#), None);
        assert_eq!(parse_reply(r#"{"text":5}"#), None);
    }

    #[test]
    fn a_missing_or_non_string_language_is_none_but_the_text_survives() {
        assert_eq!(parse_reply(r#"{"text":"Hi"}"#).unwrap().language, None);
        assert_eq!(
            parse_reply(r#"{"text":"Hi","language":3}"#)
                .unwrap()
                .language,
            None
        );
    }

    #[test]
    fn new_terms_that_are_not_an_array_of_strings_are_empty() {
        assert!(parse_reply(r#"{"text":"Hi"}"#)
            .unwrap()
            .new_terms
            .is_empty());
        assert!(parse_reply(r#"{"text":"Hi","new_terms":"A"}"#)
            .unwrap()
            .new_terms
            .is_empty());
        assert!(parse_reply(r#"{"text":"Hi","new_terms":["A",2]}"#)
            .unwrap()
            .new_terms
            .is_empty());
    }

    #[test]
    fn the_candidate_runs_from_the_first_brace_to_the_last() {
        // Two objects make one invalid candidate, which counts as no reply.
        assert_eq!(parse_reply(r#"{"text": "A"} and {"text": "B"}"#), None);
        assert_eq!(parse_reply("} {"), None);
    }

    // Ports of CodexFormatterTests.swift:199-203.

    #[test]
    fn finish_trims_newlines_and_uppercases_the_first_letter() {
        let terminal = profile_by_id("claude-code-terminal").unwrap();
        assert_eq!(finish("\nзроби це\n\n", terminal), "Зроби це");
        assert_eq!(finish("[thr] ok", generic()), "[thr] ok");
    }

    #[test]
    fn finish_keeps_leading_spaces_and_inner_blank_lines() {
        assert_eq!(finish("  hi", generic()), "  hi");
        assert_eq!(finish("first\n\nsecond", generic()), "First\n\nsecond");
    }

    #[test]
    fn finish_handles_empty_and_newline_only_text() {
        assert_eq!(finish("", generic()), "");
        assert_eq!(finish("\n\n", generic()), "");
    }
}
