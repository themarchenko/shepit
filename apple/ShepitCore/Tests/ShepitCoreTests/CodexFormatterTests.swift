import XCTest
@testable import ShepitCore

private func object(_ json: String) -> [String: Any] {
    (try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any]) ?? [:]
}

private func codexConfig(model: String = "gpt-6-luna", effort: String? = "low",
                         executable: String = "/opt/homebrew/bin/codex",
                         workDirectory: URL = URL(fileURLWithPath: "/tmp/codex-work")) -> CodexConfig {
    CodexConfig(executable: executable, model: model, reasoningEffort: effort, systemPrompt: "SYSTEM PROMPT",
                workDirectory: workDirectory)
}

final class CodexProtocolTests: XCTestCase {
    func testArgumentsLockCodexDownAndUseStdio() {
        let args = CodexProtocol.arguments()
        XCTAssertEqual(Array(args.suffix(3)), ["app-server", "--listen", "stdio://"])
        let flags = Array(args.dropLast(3))
        XCTAssertEqual(flags.count % 2, 0)
        var overrides: [String] = []
        for i in stride(from: 0, to: flags.count, by: 2) {
            XCTAssertEqual(flags[i], "-c")
            overrides.append(flags[i + 1])
        }
        for expected in ["sandbox_mode=\"read-only\"", "approval_policy=\"never\"", "features.shell_tool=false",
                         "web_search=\"disabled\"", "features.hooks=false", "features.apps=false"] {
            XCTAssertTrue(overrides.contains(expected), "missing \(expected)")
        }
    }

    func testEnvironmentPutsExecutableDirectoryFirst() {
        let env = CodexProtocol.environment(base: ["PATH": "/usr/bin:/bin", "LANG": "uk_UA.UTF-8"], home: "/Users/me",
                                            executable: "/Users/me/.nvm/versions/node/v22.1.0/bin/codex")
        XCTAssertEqual(env["PATH"], "/Users/me/.nvm/versions/node/v22.1.0/bin:/Users/me/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin")
        XCTAssertEqual(env["HOME"], "/Users/me")
        XCTAssertEqual(env["LANG"], "uk_UA.UTF-8")
    }

    func testThreadStartReplacesBaseInstructions() {
        let params = CodexProtocol.threadStartParams(codexConfig())
        XCTAssertEqual(params["baseInstructions"] as? String, "SYSTEM PROMPT")
        XCTAssertEqual(params["cwd"] as? String, "/tmp/codex-work")
        XCTAssertEqual(params["ephemeral"] as? Bool, true)
        XCTAssertEqual(params["model"] as? String, "gpt-6-luna")
        XCTAssertNil(CodexProtocol.threadStartParams(codexConfig(model: ""))["model"], "empty model uses the Codex default")
    }

    func testTurnStartCarriesTextEffortAndSchema() throws {
        let params = CodexProtocol.turnStartParams(threadID: "thr_123", text: "Transcript:\nпривіт", effort: "low")
        XCTAssertEqual(params["threadId"] as? String, "thr_123")
        let input = try XCTUnwrap(params["input"] as? [[String: Any]])
        XCTAssertEqual(input.count, 1)
        XCTAssertEqual(input[0]["type"] as? String, "text")
        XCTAssertEqual(input[0]["text"] as? String, "Transcript:\nпривіт")
        XCTAssertEqual(params["effort"] as? String, "low")
        let schema = try XCTUnwrap(params["outputSchema"] as? [String: Any])
        XCTAssertEqual(schema["required"] as? [String], ["text", "language", "new_terms"])
        XCTAssertEqual(schema["additionalProperties"] as? Bool, false)
        XCTAssertTrue(JSONSerialization.isValidJSONObject(params))
        XCTAssertNil(CodexProtocol.turnStartParams(threadID: "thr_123", text: "x", effort: nil)["effort"])
    }

    func testEncodedMessageIsOneLine() throws {
        let params = CodexProtocol.turnStartParams(threadID: "thr_123", text: "line one\nline two", effort: nil)
        let data = try CodexProtocol.encodeLine(["method": "turn/start", "id": 3, "params": params])
        XCTAssertEqual(data.last, 0x0A)
        XCTAssertEqual(data.filter { $0 == 0x0A }.count, 1)
        let decoded = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(decoded["method"] as? String, "turn/start")
    }

    // Samples in this test are copied from https://developers.openai.com/codex/app-server ("Message schema",
    // "Start or resume a thread", "Start a turn").
    func testClassifyDocumentedMessages() {
        let threadStarted = object(#"{ "id": 10, "result": { "thread": { "id": "thr_123", "sessionId": "thr_123", "preview": "", "ephemeral": false, "modelProvider": "openai", "createdAt": 1730910000 } } }"#)
        guard case .response(let id, let result, let error)? = CodexProtocol.classify(threadStarted) else {
            return XCTFail("not a response")
        }
        XCTAssertEqual(id, 10)
        XCTAssertNil(error)
        XCTAssertEqual(CodexProtocol.threadID(fromStartResult: result), "thr_123")

        let turnStarted = object(#"{ "id": 30, "result": { "turn": { "id": "turn_456", "status": "inProgress", "items": [], "error": null } } }"#)
        guard case .response(_, let turnResult, _)? = CodexProtocol.classify(turnStarted) else { return XCTFail("not a response") }
        XCTAssertEqual(CodexProtocol.turnID(fromStartResult: turnResult), "turn_456")

        let failure = object(#"{ "id": 10, "error": { "code": 123, "message": "Something went wrong" } }"#)
        guard case .response(10, _, let message?)? = CodexProtocol.classify(failure) else { return XCTFail("not an error") }
        XCTAssertEqual(message, "Something went wrong (123)")

        let notification = object(#"{ "method": "turn/started", "params": { "turn": { "id": "turn_456" } } }"#)
        guard case .notification(let method, let params)? = CodexProtocol.classify(notification) else {
            return XCTFail("not a notification")
        }
        XCTAssertEqual(method, "turn/started")
        XCTAssertEqual((params["turn"] as? [String: Any])?["id"] as? String, "turn_456")
    }

    // Reconstructed from the documented field list of `item/commandExecution/requestApproval`
    // (itemId, threadId, turnId, optional command); the docs show no full sample.
    func testServerRequestsAreDeclined() throws {
        let request = object(#"{ "method": "item/commandExecution/requestApproval", "id": 7, "params": { "threadId": "thr_123", "turnId": "turn_456", "itemId": "item_1", "command": "rm -rf build" } }"#)
        guard case .request(let id, let method, _)? = CodexProtocol.classify(request) else { return XCTFail("not a request") }
        let reply = CodexProtocol.reply(toRequest: method, id: id)
        XCTAssertEqual(reply["id"] as? Int, 7)
        XCTAssertEqual((reply["result"] as? [String: Any])?["decision"] as? String, "decline")

        let other = CodexProtocol.reply(toRequest: "item/tool/requestUserInput", id: "abc")
        XCTAssertEqual(other["id"] as? String, "abc")
        XCTAssertEqual((other["error"] as? [String: Any])?["code"] as? Int, -32601)
        XCTAssertNil(other["result"])
    }
}

/// Notification sequences reconstructed from the documented payloads ("Turn events", "Items", "Errors" on
/// https://developers.openai.com/codex/app-server) plus the threadId/turnId fields of the v2 protocol source.
final class CodexTurnCollectorTests: XCTestCase {
    private func run(_ lines: [String], turnID: String? = "turn_456") -> CodexTurnCollector.Outcome? {
        var collector = CodexTurnCollector(turnID: turnID)
        for line in lines {
            let message = object(line)
            guard let method = message["method"] as? String else { continue }
            if let outcome = collector.consume(method: method, params: message["params"] as? [String: Any] ?? [:]) {
                return outcome
            }
        }
        return nil
    }

    private let reply = #"{\"text\": \"Зроби refactoring handleSubmit\", \"language\": \"mixed\", \"new_terms\": []}"#

    func testCompletedTurnReturnsFinalAgentMessage() {
        let outcome = run([
            #"{"method":"turn/started","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"inProgress","items":[],"error":null}}}"#,
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"rs_1","type":"reasoning","summary":[],"content":[]}}}"#,
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"msg_1","type":"agentMessage","text":"Formatting now.","phase":"commentary"}}}"#,
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"msg_2","type":"agentMessage","text":"\#(reply)","phase":"final_answer"}}}"#,
            #"{"method":"thread/tokenUsage/updated","params":{"threadId":"thr_123","turnId":"turn_456","tokenUsage":{}}}"#,
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
        ])
        guard case .reply(let text)? = outcome else { return XCTFail("no reply: \(String(describing: outcome))") }
        let parsed = PromptBuilder.parseReply(text)
        XCTAssertEqual(parsed?.text, "Зроби refactoring handleSubmit")
        XCTAssertEqual(parsed?.language, "mixed")
    }

    func testMessageWithoutPhaseCounts() {
        let outcome = run([
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"msg_1","type":"agentMessage","text":"{\"text\":\"Hi\",\"language\":\"en\",\"new_terms\":[]}"}}}"#,
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
        ])
        XCTAssertEqual(outcome, .reply(#"{"text":"Hi","language":"en","new_terms":[]}"#))
    }

    func testFailedTurnCarriesErrorAndIgnoresRetries() {
        let outcome = run([
            #"{"method":"error","params":{"threadId":"thr_123","turnId":"turn_456","willRetry":true,"error":{"message":"stream disconnected","codexErrorInfo":"responseStreamDisconnected"}}}"#,
            #"{"method":"error","params":{"threadId":"thr_123","turnId":"turn_456","willRetry":false,"error":{"message":"You've hit your usage limit.","codexErrorInfo":"usageLimitExceeded"}}}"#,
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"failed","items":[],"error":null}}}"#,
        ])
        XCTAssertEqual(outcome, .failed("You've hit your usage limit. (usageLimitExceeded)"))

        let withTurnError = run([
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"failed","items":[],"error":{"message":"Unauthorized","codexErrorInfo":"unauthorized"}}}}"#,
        ])
        XCTAssertEqual(withTurnError, .failed("Unauthorized (unauthorized)"))
    }

    func testCompletedTurnWithoutMessageFails() {
        let outcome = run([
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
        ])
        XCTAssertEqual(outcome, .failed("turn completed without a reply"))
    }

    func testOtherTurnsAreIgnored() {
        let outcome = run([
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_old","item":{"id":"msg_0","type":"agentMessage","text":"stale"}}}"#,
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_old","status":"completed","items":[],"error":null}}}"#,
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_456","item":{"id":"msg_1","type":"agentMessage","text":"fresh"}}}"#,
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_456","status":"completed","items":[],"error":null}}}"#,
        ])
        XCTAssertEqual(outcome, .reply("fresh"))
    }

    func testTurnIDLearnedFromTurnStarted() {
        let outcome = run([
            #"{"method":"turn/started","params":{"threadId":"thr_123","turn":{"id":"turn_9","status":"inProgress","items":[],"error":null}}}"#,
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_8","item":{"id":"msg_0","type":"agentMessage","text":"stale"}}}"#,
            #"{"method":"item/completed","params":{"threadId":"thr_123","turnId":"turn_9","item":{"id":"msg_1","type":"agentMessage","text":"fresh"}}}"#,
            #"{"method":"turn/completed","params":{"threadId":"thr_123","turn":{"id":"turn_9","status":"completed","items":[],"error":null}}}"#,
        ], turnID: nil)
        XCTAssertEqual(outcome, .reply("fresh"))
    }
}

final class CodexFormatterFinishTests: XCTestCase {
    func testFinishMatchesClaudeCleanup() {
        XCTAssertEqual(CodexFormatter.finish("\nзроби це\n\n", profile: BuiltInProfiles.claudeCodeTerminal), "Зроби це")
        XCTAssertEqual(CodexFormatter.finish("[thr] ok", profile: BuiltInProfiles.generic), "[thr] ok")
        XCTAssertEqual(FormatterSource.codex.rawValue, "codex")
    }
}

/// Drives CodexFormatter against a stand-in for `codex app-server`: a Python script that answers the documented
/// JSON-RPC flow (initialize, thread/start, turn/start with turn/started, item/completed and turn/completed).
/// It checks the process plumbing only; the real Codex CLI is not exercised.
final class CodexFormatterProcessTests: XCTestCase {
    private var directory: URL!
    private var formatter: CodexFormatter?

    private static let fakeServer = #"""
    #!/usr/bin/python3
    import json, os, sys

    def send(message):
        sys.stdout.write(json.dumps(message) + "\n")
        sys.stdout.flush()

    def complete(thread_id, turn_id, text):
        reply = json.dumps({"text": text, "language": "en", "new_terms": []})
        send({"method": "item/completed", "params": {"threadId": thread_id, "turnId": turn_id,
              "item": {"id": "msg_" + turn_id, "type": "agentMessage", "text": reply, "phase": "final_answer"}}})
        send({"method": "turn/completed", "params": {"threadId": thread_id,
              "turn": {"id": turn_id, "status": "completed", "items": [], "error": None}}})

    with open("argv.json", "w") as f:
        json.dump(sys.argv[1:], f)
    threads = 0
    turns = 0
    waiting = None
    while True:
        line = sys.stdin.readline()
        if not line:
            break
        message = json.loads(line)
        method = message.get("method")
        if method is None:
            thread_id, turn_id = waiting
            complete(thread_id, turn_id, message.get("result", {}).get("decision", "no decision"))
            continue
        if method == "initialize":
            send({"id": message["id"], "result": {"userAgent": "fake-codex"}})
        elif method == "thread/start":
            threads += 1
            thread_id = "thr_%d_%d" % (os.getpid(), threads)
            with open("thread-start.json", "w") as f:
                json.dump(message["params"], f)
            send({"id": message["id"], "result": {"thread": {"id": thread_id, "ephemeral": True}}})
            send({"method": "thread/started", "params": {"thread": {"id": thread_id}}})
        elif method == "turn/start":
            turns += 1
            params = message["params"]
            thread_id = params["threadId"]
            turn_id = "turn_%d" % turns
            text = params["input"][0]["text"]
            turn = {"id": turn_id, "status": "inProgress", "items": [], "error": None}
            send({"id": message["id"], "result": {"turn": turn}})
            send({"method": "turn/started", "params": {"threadId": thread_id, "turn": turn}})
            transcript = text.split("Transcript:\n")[-1]
            if text.startswith("Warm-up"):
                complete(thread_id, turn_id, "ok")
            elif transcript == "hang":
                pass
            elif transcript == "exit":
                sys.exit(3)
            elif transcript == "fail":
                error = {"message": "You've hit your usage limit.", "codexErrorInfo": "usageLimitExceeded"}
                send({"method": "error", "params": {"threadId": thread_id, "turnId": turn_id, "willRetry": False, "error": error}})
                send({"method": "turn/completed", "params": {"threadId": thread_id,
                      "turn": {"id": turn_id, "status": "failed", "items": [], "error": error}}})
            elif transcript == "approve":
                waiting = (thread_id, turn_id)
                send({"method": "item/commandExecution/requestApproval", "id": 0,
                      "params": {"threadId": thread_id, "turnId": turn_id, "itemId": "item_1", "command": "touch x"}})
            else:
                complete(thread_id, turn_id, "[%s] %s" % (thread_id, transcript))
        elif "id" in message:
            send({"id": message["id"], "result": {}})
    """#

    override func setUpWithError() throws {
        guard FileManager.default.isExecutableFile(atPath: "/usr/bin/python3") else {
            throw XCTSkip("needs /usr/bin/python3 for the fake app-server")
        }
        directory = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-codex-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let script = directory.appendingPathComponent("codex")
        try Self.fakeServer.write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
    }

    override func tearDown() async throws {
        await formatter?.shutdown()
        if let directory { try? FileManager.default.removeItem(at: directory) }
    }

    private func makeFormatter(_ adjust: (inout CodexConfig) -> Void = { _ in }) -> CodexFormatter {
        var config = codexConfig(executable: directory.appendingPathComponent("codex").path, workDirectory: directory)
        config.warmUpTimeout = 10
        config.requestTimeout = 5
        adjust(&config)
        let f = CodexFormatter(config: config)
        formatter = f
        return f
    }

    private func request(_ transcript: String) -> FormatRequest {
        FormatRequest(transcript: transcript, languageHint: "en", mode: .clean, profile: BuiltInProfiles.generic,
                      context: TargetContext(bundleID: "com.apple.Notes", appName: "Notes"))
    }

    /// "[thr_<pid>_<n>] transcript" -> "thr_<pid>_<n>"
    private func thread(of result: FormatResult) -> String {
        String(result.text.dropFirst().prefix { $0 != "]" })
    }

    func testFormatsThroughAppServer() async throws {
        let f = makeFormatter()
        await f.warmUp()
        let result = try await f.format(request("hello there"))
        XCTAssertTrue(result.text.hasSuffix("] hello there"), result.text)
        XCTAssertEqual(result.source, .codex)
        XCTAssertEqual(result.language, "en")

        let argv = try JSONSerialization.jsonObject(with: Data(contentsOf: directory.appendingPathComponent("argv.json"))) as? [String]
        XCTAssertEqual(argv, CodexProtocol.arguments())
        let start = try JSONSerialization.jsonObject(with: Data(contentsOf: directory.appendingPathComponent("thread-start.json"))) as? [String: Any]
        XCTAssertEqual(start?["baseInstructions"] as? String, "SYSTEM PROMPT")
        XCTAssertEqual(start?["ephemeral"] as? Bool, true)
        XCTAssertEqual(start?["cwd"] as? String, directory.path)
    }

    func testApprovalRequestIsDeclined() async throws {
        let f = makeFormatter()
        let result = try await f.format(request("approve"))
        XCTAssertEqual(result.text, "Decline")
    }

    func testFailedTurnKeepsProcessAndThread() async throws {
        let f = makeFormatter()
        let first = try await f.format(request("one"))
        do {
            _ = try await f.format(request("fail"))
            XCTFail("expected a failure")
        } catch let error as CodexError {
            XCTAssertEqual(error, .turnFailed("You've hit your usage limit. (usageLimitExceeded)"))
        }
        let after = try await f.format(request("two"))
        XCTAssertEqual(thread(of: after), thread(of: first))
    }

    func testTimeoutRestartsProcess() async throws {
        let f = makeFormatter { $0.requestTimeout = 1 }
        let first = try await f.format(request("one"))
        do {
            _ = try await f.format(request("hang"))
            XCTFail("expected a timeout")
        } catch let error as CodexError {
            XCTAssertEqual(error, .timeout)
        }
        let after = try await f.format(request("two"))
        XCTAssertNotEqual(thread(of: after), thread(of: first), "a new process should serve the next dictation")
    }

    func testExitIsReportedAndRecovered() async throws {
        let f = makeFormatter()
        _ = try await f.format(request("one"))
        do {
            _ = try await f.format(request("exit"))
            XCTFail("expected an exit")
        } catch let error as CodexError {
            guard case .exited = error else { return XCTFail("unexpected \(error)") }
        }
        let after = try await f.format(request("two"))
        XCTAssertTrue(after.text.hasSuffix("] two"), after.text)
    }

    func testRotatesToStandbyThread() async throws {
        let f = makeFormatter {
            $0.standbyAfterTurns = 2
            $0.maxTurnsPerThread = 3
        }
        let first = try await f.format(request("a"))  // turn 2 on the first thread: standby starts
        try await Task.sleep(for: .milliseconds(500))
        let second = try await f.format(request("b"))  // turn 3: switch to the standby thread
        let third = try await f.format(request("c"))
        XCTAssertEqual(thread(of: second), thread(of: first))
        XCTAssertNotEqual(thread(of: third), thread(of: first))
        XCTAssertEqual(thread(of: third).split(separator: "_")[1], thread(of: first).split(separator: "_")[1],
                       "same process, new thread")
    }
}
