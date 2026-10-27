//! The Codex app-server wire format (CodexFormatter.swift:225-440): JSON-RPC 2.0 without the
//! `jsonrpc` field, one JSON object per line. Pure; the process is driven by `shepit-agent`.

use serde_json::{json, Map, Value};

/// The same warm-up text as Claude (CodexFormatter.swift:229).
pub const WARM_UP_MESSAGE: &str = super::claude_wire::WARM_UP_MESSAGE;

/// `-c key=value` overrides, values are TOML (CodexFormatter.swift:233-247).
pub const CONFIG_OVERRIDES: [&str; 12] = [
    "sandbox_mode=\"read-only\"",
    "approval_policy=\"never\"",
    "features.shell_tool=false",
    "web_search=\"disabled\"",
    "tools.view_image=false",
    "features.apps=false",
    "features.multi_agent=false",
    "features.hooks=false",
    "features.goals=false",
    "features.memories=false",
    "history.persistence=\"none\"",
    "model_reasoning_summary=\"none\"",
];

/// Root `-c` flags, then `app-server --listen stdio://` (CodexFormatter.swift:250-252).
pub fn arguments() -> Vec<String> {
    CONFIG_OVERRIDES
        .iter()
        .flat_map(|o| ["-c", o])
        .chain(["app-server", "--listen", "stdio://"])
        .map(String::from)
        .collect()
}

/// The PATH for the child: the executable's own folder first, then the inherited PATH, joined
/// with `;`. An npm install of Codex is a Node script and nvm keeps `node` beside it
/// (CodexFormatter.swift:262-268). An executable path without a folder leaves PATH as it was.
pub fn child_path(executable: &str, inherited: Option<&str>) -> String {
    let inherited = inherited.unwrap_or("");
    let Some(cut) = executable.rfind(['\\', '/']) else {
        return inherited.to_string();
    };
    let own = &executable[..cut];
    if inherited.is_empty() {
        own.to_string()
    } else {
        format!("{own};{inherited}")
    }
}

/// Notifications the client opts out of: deltas repeat what `item/completed` delivers whole.
pub const SKIPPED_NOTIFICATIONS: [&str; 4] = [
    "item/agentMessage/delta",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/textDelta",
];

/// `initialize` params: `clientInfo` {name shepit, title Shepit, version} and
/// `capabilities.optOutNotificationMethods`.
pub fn initialize_params(version: &str) -> Value {
    json!({
        "clientInfo": {"name": "shepit", "title": "Shepit", "version": version},
        "capabilities": {"optOutNotificationMethods": SKIPPED_NOTIFICATIONS},
    })
}

/// `thread/start` params: `cwd`, `baseInstructions` (the system prompt), `ephemeral` true and, only
/// when `model` is not empty, `model`.
pub fn thread_start_params(model: &str, system_prompt: &str, work_dir: &str) -> Value {
    let mut params = json!({
        "cwd": work_dir,
        "baseInstructions": system_prompt,
        "ephemeral": true,
    });
    if !model.is_empty() {
        params["model"] = json!(model);
    }
    params
}

/// `turn/start` params: `threadId`, `input` with one text item, `outputSchema`, and `effort` only
/// when given and not empty.
pub fn turn_start_params(thread_id: &str, text: &str, effort: Option<&str>) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "input": [{"type": "text", "text": text}],
        "outputSchema": reply_schema(),
    });
    if let Some(effort) = effort.filter(|e| !e.is_empty()) {
        params["effort"] = json!(effort);
    }
    params
}

/// The reply shape the prompt asks for; strict structured output needs every key required and no
/// extras (CodexFormatter.swift:291-301).
pub fn reply_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": "string"},
            "language": {"type": "string", "enum": ["uk", "en", "mixed"]},
            "new_terms": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["text", "language", "new_terms"],
        "additionalProperties": false,
    })
}

/// One message as a line: compact JSON, slashes unescaped, a single trailing `\n`.
pub fn encode_line(message: &Value) -> String {
    // serde_json writes compact output and leaves "/" alone.
    let mut line = message.to_string();
    line.push('\n');
    line
}

#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// `error` is `message (code)` when the code is a number, else the message.
    Response {
        id: i64,
        result: Map<String, Value>,
        error: Option<String>,
    },
    /// A request from the server; it expects an answer with the same id (a number or a string).
    Request {
        id: Value,
        method: String,
        params: Map<String, Value>,
    },
    Notification {
        method: String,
        params: Map<String, Value>,
    },
}

/// A message with `method` and a non-null `id` is a request, with `method` alone a notification;
/// otherwise it is a response and needs an integer `id`.
pub fn classify(message: &Value) -> Option<Incoming> {
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        let method = method.to_string();
        return Some(match message.get("id") {
            Some(id) if !id.is_null() => Incoming::Request {
                id: id.clone(),
                method,
                params,
            },
            _ => Incoming::Notification { method, params },
        });
    }
    let id = message.get("id")?.as_i64()?;
    if let Some(error) = message.get("error").and_then(Value::as_object) {
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        let text = match error.get("code").and_then(Value::as_i64) {
            Some(code) => format!("{text} ({code})"),
            None => text.to_string(),
        };
        return Some(Incoming::Response {
            id,
            result: Map::new(),
            error: Some(text),
        });
    }
    Some(Incoming::Response {
        id,
        result: message
            .get("result")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        error: None,
    })
}

pub fn thread_id_from_start_result(result: &Map<String, Value>) -> Option<String> {
    nested_id(result, "thread")
}

pub fn turn_id_from_start_result(result: &Map<String, Value>) -> Option<String> {
    nested_id(result, "turn")
}

fn nested_id(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)?.get("id")?.as_str().map(String::from)
}

/// Approvals are declined; anything else gets a JSON-RPC error `-32601`.
pub fn reply_to_request(method: &str, id: &Value) -> Value {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            json!({"id": id, "result": {"decision": "decline"}})
        }
        _ => json!({
            "id": id,
            "error": {"code": -32601, "message": format!("Shepit does not handle {method}")},
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Reply(String),
    Failed(String),
}

/// Folds the notifications of one turn into its result (CodexFormatter.swift:371-440).
#[derive(Debug, Default)]
pub struct TurnCollector {
    /// Known once `turn/start` answers or `turn/started` arrives; notifications of other turns
    /// are ignored from then on.
    pub turn_id: Option<String>,
    /// `(text, phase)` of each agent message, in arrival order.
    messages: Vec<(String, Option<String>)>,
    last_error: Option<String>,
}

impl TurnCollector {
    pub fn new(turn_id: Option<&str>) -> Self {
        Self {
            turn_id: turn_id.map(str::to_string),
            messages: Vec::new(),
            last_error: None,
        }
    }

    /// The outcome once the turn has completed; `None` while it runs.
    pub fn consume(&mut self, method: &str, params: &Map<String, Value>) -> Option<Outcome> {
        if let (Some(mine), Some(other)) = (&self.turn_id, turn_id_in(params)) {
            if mine != other {
                return None;
            }
        }
        match method {
            "turn/started" => {
                if self.turn_id.is_none() {
                    self.turn_id = turn_id_in(params).map(String::from);
                }
            }
            "item/completed" => {
                let item = params.get("item")?.as_object()?;
                if item.get("type")?.as_str()? != "agentMessage" {
                    return None;
                }
                let text = item.get("text")?.as_str()?.to_string();
                let phase = item.get("phase").and_then(Value::as_str).map(String::from);
                self.messages.push((text, phase));
            }
            "error" => {
                // Codex retries transient errors itself; those do not end the turn.
                if params.get("willRetry").and_then(Value::as_bool) == Some(true) {
                    return None;
                }
                if let Some(error) = params.get("error").and_then(Value::as_object) {
                    self.last_error = Some(describe(error));
                }
            }
            "turn/completed" => {
                let turn = params.get("turn").and_then(Value::as_object);
                let status = turn
                    .and_then(|t| t.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if status == "completed" {
                    return Some(match self.final_text() {
                        Some(t) if !t.trim().is_empty() => Outcome::Reply(t.to_string()),
                        _ => Outcome::Failed("turn completed without a reply".into()),
                    });
                }
                if let Some(error) = turn.and_then(|t| t.get("error")).and_then(Value::as_object) {
                    return Some(Outcome::Failed(describe(error)));
                }
                return Some(Outcome::Failed(
                    self.last_error
                        .clone()
                        .unwrap_or_else(|| format!("turn {status}")),
                ));
            }
            _ => {}
        }
        None
    }

    /// The answer rather than commentary the model wrote on the way.
    fn final_text(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|(_, phase)| phase.as_deref() != Some("commentary"))
            .or_else(|| self.messages.last())
            .map(|(text, _)| text.as_str())
    }
}

fn turn_id_in(params: &Map<String, Value>) -> Option<&str> {
    if let Some(id) = params.get("turnId").and_then(Value::as_str) {
        return Some(id);
    }
    params.get("turn")?.get("id")?.as_str()
}

fn describe(error: &Map<String, Value>) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    match error.get("codexErrorInfo").and_then(Value::as_str) {
        Some(info) => format!("{message} ({info})"),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polish::reply::parse_reply;
    use serde_json::json;

    fn object(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    // Ports of CodexProtocolTests (CodexFormatterTests.swift:16-130).

    #[test]
    fn arguments_lock_codex_down_and_use_stdio() {
        let args = arguments();
        assert_eq!(
            args[args.len() - 3..],
            ["app-server", "--listen", "stdio://"]
        );
        let flags = &args[..args.len() - 3];
        assert_eq!(flags.len() % 2, 0);
        let mut overrides = Vec::new();
        for pair in flags.chunks(2) {
            assert_eq!(pair[0], "-c");
            overrides.push(pair[1].as_str());
        }
        for want in [
            "sandbox_mode=\"read-only\"",
            "approval_policy=\"never\"",
            "features.shell_tool=false",
            "web_search=\"disabled\"",
            "features.hooks=false",
            "features.apps=false",
        ] {
            assert!(overrides.contains(&want), "missing {want}");
        }
    }

    #[test]
    fn the_overrides_are_the_swift_list_in_order() {
        let args = arguments();
        let overrides: Vec<&str> = args[..args.len() - 3]
            .chunks(2)
            .map(|p| p[1].as_str())
            .collect();
        assert_eq!(overrides, CONFIG_OVERRIDES);
        assert_eq!(CONFIG_OVERRIDES.len(), 12);
    }

    #[test]
    fn the_executable_folder_goes_first_on_path() {
        assert_eq!(
            child_path(
                r"C:\Users\me\AppData\Roaming\npm\codex.cmd",
                Some(r"C:\Windows\System32;C:\Windows")
            ),
            r"C:\Users\me\AppData\Roaming\npm;C:\Windows\System32;C:\Windows"
        );
    }

    #[test]
    fn a_missing_path_leaves_only_the_executable_folder() {
        assert_eq!(child_path(r"C:\x\codex.exe", None), r"C:\x");
        assert_eq!(child_path(r"C:\x\codex.exe", Some("")), r"C:\x");
    }

    #[test]
    fn a_bare_executable_name_leaves_path_alone() {
        assert_eq!(child_path("codex.exe", Some(r"C:\a")), r"C:\a");
    }

    #[test]
    fn thread_start_replaces_the_base_instructions() {
        let p = thread_start_params("gpt-6-luna", "SYSTEM PROMPT", r"C:\w\codex-work");
        assert_eq!(p["baseInstructions"], "SYSTEM PROMPT");
        assert_eq!(p["cwd"], r"C:\w\codex-work");
        assert_eq!(p["ephemeral"], true);
        assert_eq!(p["model"], "gpt-6-luna");
        let p = thread_start_params("", "S", "w");
        assert!(
            p.get("model").is_none(),
            "an empty model uses the Codex default"
        );
    }

    #[test]
    fn turn_start_carries_text_effort_and_schema() {
        let p = turn_start_params("thr_123", "Transcript:\nпривіт", Some("low"));
        assert_eq!(p["threadId"], "thr_123");
        let input = p["input"].as_array().unwrap();
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "text");
        assert_eq!(input[0]["text"], "Transcript:\nпривіт");
        assert_eq!(p["effort"], "low");
        assert_eq!(
            p["outputSchema"]["required"],
            json!(["text", "language", "new_terms"])
        );
        assert_eq!(p["outputSchema"]["additionalProperties"], false);
        assert!(turn_start_params("thr_123", "x", None)
            .get("effort")
            .is_none());
        assert!(turn_start_params("thr_123", "x", Some(""))
            .get("effort")
            .is_none());
    }

    #[test]
    fn the_reply_schema_allows_the_three_languages() {
        let s = reply_schema();
        assert_eq!(s["type"], "object");
        assert_eq!(
            s["properties"]["language"]["enum"],
            json!(["uk", "en", "mixed"])
        );
        assert_eq!(s["properties"]["new_terms"]["items"]["type"], "string");
    }

    #[test]
    fn initialize_names_the_client_and_opts_out_of_deltas() {
        let p = initialize_params("0.4.0");
        assert_eq!(
            p["clientInfo"],
            json!({"name":"shepit","title":"Shepit","version":"0.4.0"})
        );
        assert_eq!(
            p["capabilities"]["optOutNotificationMethods"],
            json!(SKIPPED_NOTIFICATIONS)
        );
    }

    #[test]
    fn an_encoded_message_is_one_line() {
        let params = turn_start_params("thr_123", "line one\nline two", None);
        let line = encode_line(&json!({"method":"turn/start","id":3,"params":params}));
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1, "{line:?}");
        let decoded: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(decoded["method"], "turn/start");
    }

    #[test]
    fn slashes_are_not_escaped_in_an_encoded_line() {
        let line = encode_line(&json!({"cwd":"C:/work/a"}));
        assert!(line.contains("C:/work/a"), "{line}");
    }

    // Samples copied from https://developers.openai.com/codex/app-server, as in the Swift test.
    #[test]
    fn documented_messages_are_classified() {
        let thread_started = object(
            r#"{ "id": 10, "result": { "thread": { "id": "thr_123", "sessionId": "thr_123", "preview": "", "ephemeral": false, "modelProvider": "openai", "createdAt": 1730910000 } } }"#,
        );
        let Some(Incoming::Response { id, result, error }) = classify(&thread_started) else {
            panic!("not a response");
        };
        assert_eq!(id, 10);
        assert_eq!(error, None);
        assert_eq!(
            thread_id_from_start_result(&result).as_deref(),
            Some("thr_123")
        );

        let turn_started = object(
            r#"{ "id": 30, "result": { "turn": { "id": "turn_456", "status": "inProgress", "items": [], "error": null } } }"#,
        );
        let Some(Incoming::Response { result, .. }) = classify(&turn_started) else {
            panic!("not a response");
        };
        assert_eq!(
            turn_id_from_start_result(&result).as_deref(),
            Some("turn_456")
        );

        let failure =
            object(r#"{ "id": 10, "error": { "code": 123, "message": "Something went wrong" } }"#);
        let Some(Incoming::Response {
            id: 10,
            error: Some(message),
            ..
        }) = classify(&failure)
        else {
            panic!("not an error");
        };
        assert_eq!(message, "Something went wrong (123)");

        let notification =
            object(r#"{ "method": "turn/started", "params": { "turn": { "id": "turn_456" } } }"#);
        let Some(Incoming::Notification { method, params }) = classify(&notification) else {
            panic!("not a notification");
        };
        assert_eq!(method, "turn/started");
        assert_eq!(params["turn"]["id"], "turn_456");
    }

    #[test]
    fn an_error_without_a_code_is_just_the_message() {
        let Some(Incoming::Response { error, .. }) =
            classify(&object(r#"{"id":1,"error":{"message":"nope"}}"#))
        else {
            panic!("not a response");
        };
        assert_eq!(error.as_deref(), Some("nope"));
    }

    #[test]
    fn a_message_without_method_or_integer_id_is_not_classified() {
        assert_eq!(classify(&object(r#"{"result":{}}"#)), None);
        assert_eq!(classify(&object(r#"{"id":"x","result":{}}"#)), None);
    }

    // Reconstructed from the documented field list, as in the Swift test.
    #[test]
    fn server_requests_are_declined() {
        let request = object(
            r#"{ "method": "item/commandExecution/requestApproval", "id": 7, "params": { "threadId": "thr_123", "turnId": "turn_456", "itemId": "item_1", "command": "rm -rf build" } }"#,
        );
        let Some(Incoming::Request { id, method, .. }) = classify(&request) else {
            panic!("not a request");
        };
        let reply = reply_to_request(&method, &id);
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["result"]["decision"], "decline");

        let other = reply_to_request("item/tool/requestUserInput", &json!("abc"));
        assert_eq!(other["id"], "abc");
        assert_eq!(other["error"]["code"], -32601);
        assert!(other.get("result").is_none());
    }

    #[test]
    fn file_change_approvals_are_declined_too() {
        let reply = reply_to_request("item/fileChange/requestApproval", &json!(9));
        assert_eq!(reply["result"]["decision"], "decline");
    }

    // Ports of CodexTurnCollectorTests (CodexFormatterTests.swift:132-190).

    fn run(lines: &[&str], turn_id: Option<&str>) -> Option<Outcome> {
        let mut collector = TurnCollector::new(turn_id);
        for line in lines {
            let message = object(line);
            let Some(method) = message["method"].as_str() else {
                continue;
            };
            let params = message["params"].as_object().cloned().unwrap_or_default();
            if let Some(outcome) = collector.consume(method, &params) {
                return Some(outcome);
            }
        }
        None
    }

    const REPLY: &str = r#"{\"text\": \"Зроби refactoring handleSubmit\", \"language\": \"mixed\", \"new_terms\": []}"#;

    #[test]
    fn a_completed_turn_returns_the_final_agent_message() {
        let final_message = format!(
            r#"{{"method":"item/completed","params":{{"threadId":"thr_123","turnId":"turn_456","item":{{"id":"msg_2","type":"agentMessage","text":"{REPLY}","phase":"final_answer"}}}}}}"#
        );
        let outcome = run(
            &[
                r#"{"method":"turn/started","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"inProgress","items":[],"error":null}}}"#,
                r#"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"rs_1","type":"reasoning","summary":[],"content":[]}}}"#,
                r#"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"msg_1","type":"agentMessage","text":"Formatting now.","phase":"commentary"}}}"#,
                &final_message,
                r#"{"method":"thread/tokenUsage/updated","params":{"threadId":"thr_123","turnId":"turn_456","tokenUsage":{}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        let Some(Outcome::Reply(text)) = outcome else {
            panic!("no reply: {outcome:?}");
        };
        let parsed = parse_reply(&text).unwrap();
        assert_eq!(parsed.text, "Зроби refactoring handleSubmit");
        assert_eq!(parsed.language.as_deref(), Some("mixed"));
    }

    #[test]
    fn a_message_without_a_phase_counts() {
        let outcome = run(
            &[
                r#"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"msg_1","type":"agentMessage","text":"{\"text\":\"Hi\",\"language\":\"en\",\"new_terms\":[]}"}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(
            outcome,
            Some(Outcome::Reply(
                r#"{"text":"Hi","language":"en","new_terms":[]}"#.into()
            ))
        );
    }

    #[test]
    fn a_failed_turn_carries_the_error_and_ignores_retries() {
        let outcome = run(
            &[
                r#"{"method":"error","params":{"threadId":"thr_123","turnId":"turn_456","willRetry":true,"error":{"message":"stream disconnected","codexErrorInfo":"responseStreamDisconnected"}}}"#,
                r#"{"method":"error","params":{"threadId":"thr_123","turnId":"turn_456","willRetry":false,"error":{"message":"You've hit your usage limit.","codexErrorInfo":"usageLimitExceeded"}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"failed","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(
            outcome,
            Some(Outcome::Failed(
                "You've hit your usage limit. (usageLimitExceeded)".into()
            ))
        );

        let with_turn_error = run(
            &[
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"failed","items":[],"error":{"message":"Unauthorized","codexErrorInfo":"unauthorized"}}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(
            with_turn_error,
            Some(Outcome::Failed("Unauthorized (unauthorized)".into()))
        );
    }

    #[test]
    fn a_failed_turn_without_any_error_names_its_status() {
        let outcome = run(
            &[
                r#"{"method":"turn/completed","params":{"threadId":"t","turn":{"id":"turn_456","status":"interrupted","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(outcome, Some(Outcome::Failed("turn interrupted".into())));
    }

    #[test]
    fn a_completed_turn_without_a_message_fails() {
        let outcome = run(
            &[
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(
            outcome,
            Some(Outcome::Failed("turn completed without a reply".into()))
        );
    }

    #[test]
    fn a_blank_final_message_counts_as_no_reply() {
        let outcome = run(
            &[
                r#"{"method":"item/completed","params":{"threadId":"t","turnId":"turn_456","item":{"id":"m","type":"agentMessage","text":" \n "}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"t","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(
            outcome,
            Some(Outcome::Failed("turn completed without a reply".into()))
        );
    }

    #[test]
    fn commentary_is_used_only_when_nothing_else_arrived() {
        let outcome = run(
            &[
                r#"{"method":"item/completed","params":{"threadId":"t","turnId":"turn_456","item":{"id":"m","type":"agentMessage","text":"only commentary","phase":"commentary"}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"t","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(outcome, Some(Outcome::Reply("only commentary".into())));
    }

    #[test]
    fn other_turns_are_ignored() {
        let outcome = run(
            &[
                r#"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_old","item":{"id":"msg_0","type":"agentMessage","text":"stale"}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_old","status":"completed","items":[],"error":null}}}"#,
                r#"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"msg_1","type":"agentMessage","text":"fresh"}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(outcome, Some(Outcome::Reply("fresh".into())));
    }

    #[test]
    fn the_turn_id_is_learned_from_turn_started() {
        let outcome = run(
            &[
                r#"{"method":"turn/started","params":{"threadId":"thr_123","turn":{"id":"turn_9","status":"inProgress","items":[],"error":null}}}"#,
                r#"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_8","item":{"id":"msg_0","type":"agentMessage","text":"stale"}}}"#,
                r#"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_9","item":{"id":"msg_1","type":"agentMessage","text":"fresh"}}}"#,
                r#"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_9","status":"completed","items":[],"error":null}}}"#,
            ],
            None,
        );
        assert_eq!(outcome, Some(Outcome::Reply("fresh".into())));
    }

    #[test]
    fn other_notification_methods_do_not_end_the_turn() {
        let outcome = run(
            &[
                r#"{"method":"thread/tokenUsage/updated","params":{"threadId":"t","turnId":"turn_456","tokenUsage":{}}}"#,
            ],
            Some("turn_456"),
        );
        assert_eq!(outcome, None);
    }
}
