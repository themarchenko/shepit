//! The Claude Code stream-json wire format (ClaudeCodeWorker.swift). Pure encoding and decoding;
//! the process is driven by the `shepit-agent` crate.

use super::pipeline::PolishError;
use serde_json::Value;
use std::collections::BTreeMap;

/// The first turn of a conversation, which creates the prompt cache (ClaudeCodeWorker.swift:280).
pub const WARM_UP_MESSAGE: &str =
    "Warm-up. Return exactly: {\"text\": \"ok\", \"language\": \"en\", \"new_terms\": []}";

/// Variables removed from the child's environment (ClaudeCodeWorker.swift:70).
pub const SCRUBBED_ENV: [&str; 3] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SSE_PORT",
];

/// The argv of one worker (ClaudeCodeWorker.swift:62-67). `effort` is `low`. Holds only constants,
/// the model id and two paths the app owns: no user text.
pub fn args(model: &str, effort: &str, system_prompt_file: &str, debug_file: &str) -> Vec<String> {
    let _ = (model, effort, system_prompt_file, debug_file);
    todo!("task 9 chunk A: claude args")
}

/// One user turn as a line of stdin: a JSON object `{"type":"user","message":{"role":"user",
/// "content":<content>}}` and a single trailing `\n`.
pub fn user_line(content: &str) -> String {
    let _ = content;
    todo!("task 9 chunk A: claude user_line")
}

/// A `result` event (ClaudeCodeWorker.swift:104-112, 180-186).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeResult {
    pub text: String,
    pub is_error: bool,
    pub subtype: Option<String>,
    /// Only the integer values of `usage`.
    pub usage: BTreeMap<String, i64>,
}

/// `Some` for an event with `type` `result`; `None` for any other event. A missing `result`
/// reads as empty text and a missing `is_error` as false.
pub fn decode_result(event: &Value) -> Option<ClaudeResult> {
    let _ = event;
    todo!("task 9 chunk A: decode_result")
}

impl ClaudeResult {
    /// The reply text, or `PolishError::Api` for an error result: the text when it is not empty,
    /// else the subtype, else `unknown`.
    pub fn into_text(self) -> Result<String, PolishError> {
        todo!("task 9 chunk A: ClaudeResult::into_text")
    }
}

/// Splits process output into JSON objects: lines end at `\n`, a trailing `\r` is dropped, and a
/// line that is empty or not a JSON object is skipped (ClaudeCodeWorker.swift:101-107). Bytes are
/// buffered, so a chunk may end inside a line or inside a multi-byte character.
#[derive(Debug, Default)]
#[allow(dead_code)] // fields are read once the logic lands
pub struct LineBuffer {
    pending: Vec<u8>,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a chunk and returns the objects of every line it completed.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Value> {
        let _ = bytes;
        todo!("task 9 chunk A: LineBuffer::push")
    }

    /// The stream ended: a last line without `\n` still counts.
    pub fn finish(&mut self) -> Vec<Value> {
        todo!("task 9 chunk A: LineBuffer::finish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_argv_equals_the_swift_one() {
        assert_eq!(
            args(
                "haiku",
                "low",
                r"C:\w\system-prompt.md",
                r"C:\w\worker-debug.log"
            ),
            [
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--model",
                "haiku",
                "--effort",
                "low",
                "--tools",
                "",
                "--setting-sources",
                "",
                "--strict-mcp-config",
                "--no-session-persistence",
                "--system-prompt-file",
                r"C:\w\system-prompt.md",
                "--debug-file",
                r"C:\w\worker-debug.log",
            ]
        );
    }

    #[test]
    fn the_model_goes_into_the_argv() {
        let a = args("sonnet", "low", "p", "d");
        let i = a.iter().position(|x| x == "--model").unwrap();
        assert_eq!(a[i + 1], "sonnet");
    }

    #[test]
    fn the_empty_tools_and_setting_sources_arguments_survive() {
        let a = args("haiku", "low", "p", "d");
        for flag in ["--tools", "--setting-sources"] {
            let i = a.iter().position(|x| x == flag).unwrap();
            assert_eq!(a[i + 1], "", "{flag}");
        }
    }

    #[test]
    fn the_user_line_is_one_json_object_and_one_newline() {
        let line = user_line("first\nsecond");
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1, "{line:?}");
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            v,
            json!({"type":"user","message":{"role":"user","content":"first\nsecond"}})
        );
    }

    #[test]
    fn cyrillic_and_quotes_survive_the_user_line() {
        let text = "Привіт, \"світе\" \\ / 😀";
        let v: Value = serde_json::from_str(user_line(text).trim_end()).unwrap();
        assert_eq!(v["message"]["content"], text);
    }

    #[test]
    fn the_warm_up_message_is_the_swift_text() {
        assert_eq!(
            WARM_UP_MESSAGE,
            "Warm-up. Return exactly: {\"text\": \"ok\", \"language\": \"en\", \"new_terms\": []}"
        );
    }

    #[test]
    fn the_scrubbed_variables_are_the_three_claude_ones() {
        assert_eq!(
            SCRUBBED_ENV,
            [
                "CLAUDECODE",
                "CLAUDE_CODE_ENTRYPOINT",
                "CLAUDE_CODE_SSE_PORT"
            ]
        );
    }

    #[test]
    fn a_result_event_is_decoded() {
        let ev = json!({"type":"result","subtype":"success","is_error":false,"result":"{\"text\":\"Hi\"}",
                        "usage":{"input_tokens":3,"output_tokens":5,"cache_read_input_tokens":100}});
        let r = decode_result(&ev).unwrap();
        assert_eq!(r.text, "{\"text\":\"Hi\"}");
        assert!(!r.is_error);
        assert_eq!(r.subtype.as_deref(), Some("success"));
        assert_eq!(r.usage.get("input_tokens"), Some(&3));
        assert_eq!(r.usage.get("cache_read_input_tokens"), Some(&100));
    }

    #[test]
    fn only_integer_usage_values_are_kept() {
        let ev = json!({"type":"result","result":"x","usage":{"a":1,"b":"two","c":{"d":4},"e":2.5,"f":null}});
        let r = decode_result(&ev).unwrap();
        assert_eq!(r.usage.keys().collect::<Vec<_>>(), ["a"]);
    }

    #[test]
    fn other_events_are_not_results() {
        assert_eq!(
            decode_result(&json!({"type":"assistant","message":{}})),
            None
        );
        assert_eq!(
            decode_result(&json!({"type":"system","subtype":"init"})),
            None
        );
        assert_eq!(decode_result(&json!({"result":"x"})), None);
        assert_eq!(decode_result(&json!({"type":5})), None);
    }

    #[test]
    fn a_result_with_missing_fields_has_empty_text_and_no_error() {
        let r = decode_result(&json!({"type":"result"})).unwrap();
        assert_eq!((r.text.as_str(), r.is_error, r.subtype), ("", false, None));
        assert!(r.usage.is_empty());
    }

    #[test]
    fn a_good_result_gives_its_text() {
        let r = decode_result(&json!({"type":"result","result":"reply"})).unwrap();
        assert_eq!(r.into_text(), Ok("reply".to_string()));
    }

    #[test]
    fn an_error_result_is_an_api_error_with_the_text() {
        let r = decode_result(&json!({"type":"result","is_error":true,"result":"Invalid API key","subtype":"error_during_execution"})).unwrap();
        assert_eq!(
            r.into_text(),
            Err(PolishError::Api("Invalid API key".into()))
        );
    }

    #[test]
    fn an_error_result_without_text_uses_the_subtype_then_unknown() {
        let r = decode_result(
            &json!({"type":"result","is_error":true,"result":"","subtype":"error_max_turns"}),
        )
        .unwrap();
        assert_eq!(
            r.into_text(),
            Err(PolishError::Api("error_max_turns".into()))
        );
        let r = decode_result(&json!({"type":"result","is_error":true})).unwrap();
        assert_eq!(r.into_text(), Err(PolishError::Api("unknown".into())));
    }

    #[test]
    fn complete_lines_become_objects() {
        let mut b = LineBuffer::new();
        let out = b.push(b"{\"a\":1}\n{\"b\":2}\n");
        assert_eq!(out, [json!({"a":1}), json!({"b":2})]);
    }

    #[test]
    fn a_line_split_across_chunks_is_joined() {
        let mut b = LineBuffer::new();
        assert!(b.push(b"{\"a\":").is_empty());
        assert_eq!(b.push(b"1}\n"), [json!({"a":1})]);
    }

    #[test]
    fn a_multi_byte_character_split_across_chunks_survives() {
        let line = "{\"t\":\"Привіт\"}\n".as_bytes();
        let cut = line.iter().position(|&x| x == 0xD0).unwrap() + 1;
        let mut b = LineBuffer::new();
        assert!(b.push(&line[..cut]).is_empty());
        assert_eq!(b.push(&line[cut..]), [json!({"t":"Привіт"})]);
    }

    #[test]
    fn crlf_blank_and_non_object_lines_are_skipped() {
        let mut b = LineBuffer::new();
        let out = b.push(b"{\"a\":1}\r\n\r\n\nnot json\n[1,2]\n\"str\"\n42\n{\"b\":2}\n");
        assert_eq!(out, [json!({"a":1}), json!({"b":2})]);
    }

    #[test]
    fn an_unterminated_last_line_comes_out_at_finish() {
        let mut b = LineBuffer::new();
        assert!(b.push(b"{\"a\":1}\n{\"b\":2}").len() == 1);
        assert_eq!(b.finish(), [json!({"b":2})]);
        assert!(b.finish().is_empty());
    }

    #[test]
    fn an_unterminated_garbage_tail_is_dropped_at_finish() {
        let mut b = LineBuffer::new();
        b.push(b"{\"a\":");
        assert!(b.finish().is_empty());
    }
}
