import Foundation

/// Settings for the Codex backend. One `codex app-server` process serves every dictation.
public struct CodexConfig: Sendable {
    /// OpenAI recommends Luna for short, repeatable transformations, which is what polishing is.
    public static let defaultModel = "gpt-6-luna"

    public var executable: String
    /// Model slug. Empty uses the default model from the user's Codex config.
    public var model: String
    /// Reasoning effort sent with every turn; nil keeps the model default. Formatting is routine work, so low keeps latency down.
    public var reasoningEffort: String?
    /// Replaces Codex's built-in coding-agent instructions.
    public var systemPrompt: String
    public var workDirectory: URL
    /// Turns on one thread before a fresh one takes over, so the context never grows unbounded.
    public var maxTurnsPerThread: Int = 25
    public var standbyAfterTurns: Int = 20
    public var requestTimeout: TimeInterval = 25
    /// Covers process start, the handshake, the thread start and the first turn.
    public var warmUpTimeout: TimeInterval = 40

    public init(executable: String, model: String = CodexConfig.defaultModel, reasoningEffort: String? = "low",
                systemPrompt: String, workDirectory: URL) {
        self.executable = executable
        self.model = model
        self.reasoningEffort = reasoningEffort
        self.systemPrompt = systemPrompt
        self.workDirectory = workDirectory
    }
}

public enum CodexError: Error, LocalizedError, Equatable {
    case notRunning
    case timeout
    case exited(Int32)
    case rpc(String)
    case turnFailed(String)
    case badReply(bytes: Int)
    case paused(until: Date)

    public var errorDescription: String? {
        switch self {
        case .notRunning: return "Codex is not running"
        case .timeout: return "Codex timed out"
        case .exited(let code): return code < 0 ? "Codex exited" : "Codex exited with code \(code)"
        case .rpc(let s): return "Codex error: \(s)"
        case .turnFailed(let s): return "Codex error: \(s)"
        case .badReply(let n): return "Codex reply was not JSON (\(n) bytes)"
        case .paused(let until): return "Codex paused after repeated failures until \(until.formatted(date: .omitted, time: .standard))"
        }
    }
}

/// Polishes dictations through a long-lived `codex app-server` process (JSON-RPC over stdio).
///
/// A dictation costs one `turn/start` on a warm thread: no process start, no session start, and the
/// prompt cache and the connection stay warm. After `maxTurnsPerThread` turns a standby thread, warmed in
/// the background on the same process, takes over, the way the Claude worker recycles its process.
public actor CodexFormatter: Formatter {
    private let config: CodexConfig
    private var server: CodexAppServerProcess?
    private var activeThread: String?
    private var activeTurns = 0
    private var standbyThread: String?
    private var standbyTask: Task<Void, Never>?
    private var starting: Task<String, Error>?
    private var busy = false
    private var consecutiveFailures = 0
    private var pausedUntil: Date?

    public init(config: CodexConfig) {
        self.config = config
    }

    public func warmUp() async {
        do {
            _ = try await ensureThread()
        } catch {
            Log.formatter.error("codex warm-up failed: \(FormattingService.logKind(error), privacy: .public)")
        }
    }

    public func format(_ request: FormatRequest) async throws -> FormatResult {
        while busy { try await Task.sleep(for: .milliseconds(50)) }
        busy = true
        defer { busy = false }
        let threadID = try await ensureThread()
        guard let server else { throw CodexError.notRunning }
        let message = PromptBuilder.userMessage(request)
        let effort = config.reasoningEffort
        let t0 = Date()
        let reply: String
        do {
            reply = try await codexDeadline(config.requestTimeout) {
                try await server.runTurn(threadID: threadID, text: message, effort: effort)
            }
        } catch CodexError.turnFailed(let reason) {
            // Codex itself reported the failure (usage limit, upstream error): the process is healthy, keep it.
            throw CodexError.turnFailed(reason)
        } catch {
            // After a timeout or an exit the process is in an unknown state; drop it.
            dropServer()
            throw error
        }
        activeTurns += 1
        rotateIfNeeded()
        guard let parsed = PromptBuilder.parseReply(reply) else { throw CodexError.badReply(bytes: reply.utf8.count) }
        return FormatResult(text: Self.finish(parsed.text, profile: request.profile), language: parsed.language,
                            newTerms: parsed.newTerms, source: .codex, latency: Date().timeIntervalSince(t0))
    }

    public func shutdown() {
        dropServer()
    }

    /// Same cleanup as the Claude worker, so both backends insert text of the same shape.
    static func finish(_ raw: String, profile: Profile) -> String {
        var text = raw.trimmingCharacters(in: .newlines)
        if profile.stripTrailingNewline {
            while text.hasSuffix("\n") { text.removeLast() }
        }
        if let first = text.first, first.isLetter, first.isLowercase {
            text = first.uppercased() + text.dropFirst()
        }
        return text
    }

    // MARK: lifecycle

    private func ensureThread() async throws -> String {
        if let server, server.isAlive {
            if let activeThread { return activeThread }
            if let standbyThread {
                promoteStandby()
                return standbyThread
            }
        }
        // warmUp() and the first format() can overlap; both wait for the same start.
        if let starting { return try await starting.value }
        if let pausedUntil, pausedUntil > Date() { throw CodexError.paused(until: pausedUntil) }
        let task = Task { try await self.startServer() }
        starting = task
        defer { starting = nil }
        return try await task.value
    }

    private func startServer() async throws -> String {
        dropServer()
        let config = self.config
        let t0 = Date()
        Log.formatter.notice("spawning codex app-server (\(config.model.isEmpty ? "default model" : config.model, privacy: .public))")
        do {
            let started = try await codexDeadline(config.warmUpTimeout) { () async throws -> CodexStart in
                let server = try CodexAppServerProcess(config: config)
                do {
                    try await server.handshake()
                    let threadID = try await server.startWarmThread(config)
                    return CodexStart(server: server, threadID: threadID)
                } catch {
                    server.terminate()
                    throw error
                }
            }
            consecutiveFailures = 0
            pausedUntil = nil
            server = started.server
            activeThread = started.threadID
            activeTurns = 1
            Log.formatter.notice("codex warm in \(Date().timeIntervalSince(t0), format: .fixed(precision: 2)) s")
            return started.threadID
        } catch {
            consecutiveFailures += 1
            if consecutiveFailures >= 2 {
                pausedUntil = Date().addingTimeInterval(300)
                Log.formatter.error("codex failed \(self.consecutiveFailures, privacy: .public) times in a row; pausing Codex formatting for 5 minutes")
            }
            Log.formatter.error("codex start failed after \(Date().timeIntervalSince(t0), format: .fixed(precision: 1)) s: \(FormattingService.logKind(error), privacy: .public); see \(config.workDirectory.path, privacy: .public)/codex-stderr.log")
            throw error
        }
    }

    private func rotateIfNeeded() {
        guard let server else { return }
        if activeTurns >= config.standbyAfterTurns, standbyThread == nil, standbyTask == nil {
            standbyTask = Task { await self.prepareStandby(on: server) }
        }
        if activeTurns >= config.maxTurnsPerThread, standbyThread != nil {
            promoteStandby()
        }
    }

    private func promoteStandby() {
        guard let standby = standbyThread else { return }
        if let old = activeThread { server?.release(threadID: old) }
        activeThread = standby
        activeTurns = 1
        standbyThread = nil
        Log.formatter.notice("codex switched to the standby thread")
    }

    private func prepareStandby(on server: CodexAppServerProcess) async {
        let config = self.config
        do {
            let threadID = try await codexDeadline(config.warmUpTimeout) { try await server.startWarmThread(config) }
            if self.server === server { standbyThread = threadID }
        } catch {
            Log.formatter.error("codex standby thread failed: \(FormattingService.logKind(error), privacy: .public)")
        }
        if self.server === server { standbyTask = nil }
    }

    private func dropServer() {
        standbyTask?.cancel()
        standbyTask = nil
        server?.terminate()
        server = nil
        activeThread = nil
        standbyThread = nil
        activeTurns = 0
    }
}

private struct CodexStart: Sendable {
    let server: CodexAppServerProcess
    let threadID: String
}

// MARK: - Wire format

/// Messages of `codex app-server` (https://developers.openai.com/codex/app-server): JSON-RPC 2.0 without
/// the "jsonrpc" field, one JSON object per line. Pure functions, unit tested.
enum CodexProtocol {
    static let warmUpMessage = "Warm-up. Return exactly: {\"text\": \"ok\", \"language\": \"en\", \"new_terms\": []}"

    /// `-c key=value` overrides for the whole process; values are TOML. The formatter needs only the model,
    /// so Codex gets no command execution, no file writes, no web search, no connectors and no hooks.
    static let configOverrides: [String] = [
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
        // Dictations stay out of Codex's own history file.
        "history.persistence=\"none\"",
        // Reasoning summaries are never shown.
        "model_reasoning_summary=\"none\"",
    ]

    /// Root `-c` flags go before the subcommand; app-server receives them as config overrides.
    static func arguments() -> [String] {
        configOverrides.flatMap { ["-c", $0] } + ["app-server", "--listen", "stdio://"]
    }

    /// A Finder-launched app has a minimal PATH. The executable's own directory comes first because an npm
    /// install of Codex is a Node script and nvm keeps `node` next to it.
    static func environment(base: [String: String], home: String, executable: String) -> [String: String] {
        var env = base
        let ownDirectory = URL(fileURLWithPath: executable).deletingLastPathComponent().path
        env["PATH"] = "\(ownDirectory):\(home)/.local/bin:/opt/homebrew/bin:/usr/local/bin:" + (base["PATH"] ?? "/usr/bin:/bin")
        env["HOME"] = home
        return env
    }

    /// Deltas repeat what `item/completed` delivers whole; opting out keeps the stream small.
    static let skippedNotifications = [
        "item/agentMessage/delta",
        "item/reasoning/summaryTextDelta",
        "item/reasoning/summaryPartAdded",
        "item/reasoning/textDelta",
    ]

    static func initializeParams(version: String) -> [String: Any] {
        [
            "clientInfo": ["name": "shepit", "title": "Shepit", "version": version],
            "capabilities": ["optOutNotificationMethods": skippedNotifications],
        ]
    }

    /// `baseInstructions` replaces Codex's coding-agent prompt with the formatter prompt. Ephemeral threads
    /// keep dictations out of the saved session files.
    static func threadStartParams(_ config: CodexConfig) -> [String: Any] {
        var params: [String: Any] = [
            "cwd": config.workDirectory.path,
            "baseInstructions": config.systemPrompt,
            "ephemeral": true,
        ]
        if !config.model.isEmpty { params["model"] = config.model }
        return params
    }

    static func turnStartParams(threadID: String, text: String, effort: String?) -> [String: Any] {
        var params: [String: Any] = [
            "threadId": threadID,
            "input": [["type": "text", "text": text]],
            "outputSchema": replySchema,
        ]
        if let effort, !effort.isEmpty { params["effort"] = effort }
        return params
    }

    /// The reply shape PromptBuilder asks for. Strict structured output needs every key required and no extras.
    static let replySchema: [String: Any] = [
        "type": "object",
        "properties": [
            "text": ["type": "string"],
            "language": ["type": "string", "enum": ["uk", "en", "mixed"]],
            "new_terms": ["type": "array", "items": ["type": "string"]],
        ],
        "required": ["text", "language", "new_terms"],
        "additionalProperties": false,
    ]

    static func encodeLine(_ message: [String: Any]) throws -> Data {
        var data = try JSONSerialization.data(withJSONObject: message, options: [.withoutEscapingSlashes])
        data.append(0x0A)
        return data
    }

    enum Incoming {
        case response(id: Int, result: [String: Any], error: String?)
        /// A request from the server, which expects an answer with the same id.
        case request(id: Any, method: String, params: [String: Any])
        case notification(method: String, params: [String: Any])
    }

    static func classify(_ message: [String: Any]) -> Incoming? {
        let params = message["params"] as? [String: Any] ?? [:]
        if let method = message["method"] as? String {
            if let id = message["id"], !(id is NSNull) { return .request(id: id, method: method, params: params) }
            return .notification(method: method, params: params)
        }
        guard let id = message["id"] as? Int else { return nil }
        if let error = message["error"] as? [String: Any] {
            let text = error["message"] as? String ?? "unknown error"
            return .response(id: id, result: [:], error: (error["code"] as? Int).map { "\(text) (\($0))" } ?? text)
        }
        return .response(id: id, result: message["result"] as? [String: Any] ?? [:], error: nil)
    }

    static func threadID(fromStartResult result: [String: Any]) -> String? {
        (result["thread"] as? [String: Any])?["id"] as? String
    }

    static func turnID(fromStartResult result: [String: Any]) -> String? {
        (result["turn"] as? [String: Any])?["id"] as? String
    }

    /// Approvals are declined; anything else gets a JSON-RPC error. Either way the turn goes on without Shepit.
    static func reply(toRequest method: String, id: Any) -> [String: Any] {
        switch method {
        case "item/commandExecution/requestApproval", "item/fileChange/requestApproval":
            return ["id": id, "result": ["decision": "decline"]]
        default:
            return ["id": id, "error": ["code": -32601, "message": "Shepit does not handle \(method)"]]
        }
    }
}

/// Folds the notifications of one turn into its result.
struct CodexTurnCollector {
    enum Outcome: Equatable {
        case reply(String)
        case failed(String)
    }

    /// Known once `turn/start` answers or `turn/started` arrives; notifications of other turns are ignored then.
    var turnID: String?
    private var messages: [(text: String, phase: String?)] = []
    private var lastError: String?

    init(turnID: String? = nil) {
        self.turnID = turnID
    }

    /// The outcome once the turn has completed; nil while it is still running.
    mutating func consume(method: String, params: [String: Any]) -> Outcome? {
        if let turnID, let other = Self.turnID(in: params), other != turnID { return nil }
        switch method {
        case "turn/started":
            if turnID == nil { turnID = Self.turnID(in: params) }
        case "item/completed":
            guard let item = params["item"] as? [String: Any], item["type"] as? String == "agentMessage",
                  let text = item["text"] as? String else { return nil }
            messages.append((text, item["phase"] as? String))
        case "error":
            // Codex retries transient errors itself; those do not end the turn.
            if params["willRetry"] as? Bool == true { return nil }
            if let error = params["error"] as? [String: Any] { lastError = Self.describe(error) }
        case "turn/completed":
            let turn = params["turn"] as? [String: Any] ?? [:]
            let status = turn["status"] as? String ?? "unknown"
            if status == "completed" {
                guard let text = finalText, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                    return .failed("turn completed without a reply")
                }
                return .reply(text)
            }
            if let error = turn["error"] as? [String: Any] { return .failed(Self.describe(error)) }
            return .failed(lastError ?? "turn \(status)")
        default:
            break
        }
        return nil
    }

    /// The answer rather than commentary the model wrote on the way.
    private var finalText: String? {
        messages.last(where: { $0.phase != "commentary" })?.text ?? messages.last?.text
    }

    private static func turnID(in params: [String: Any]) -> String? {
        if let id = params["turnId"] as? String { return id }
        return (params["turn"] as? [String: Any])?["id"] as? String
    }

    static func describe(_ error: [String: Any]) -> String {
        let message = error["message"] as? String ?? "unknown error"
        if let info = error["codexErrorInfo"] as? String { return "\(message) (\(info))" }
        return message
    }
}

// MARK: - Process

private struct CodexJSON: @unchecked Sendable {
    let object: [String: Any]
}

/// One `codex app-server` child process and its JSON-RPC connection. Several threads can run turns at once;
/// notifications are routed by thread id.
final class CodexAppServerProcess: @unchecked Sendable {
    private let process = Process()
    private let stdinPipe = Pipe()
    private let stdoutPipe = Pipe()
    private let lock = NSLock()
    private let writeLock = NSLock()
    private var nextID = 1
    private var pending: [Int: CodexOneShot<CodexJSON>] = [:]
    private var turns: [String: ActiveTurn] = [:]
    private var launched = false
    private var exited = false
    private var exitStatus: Int32 = -1

    /// Guarded by the owner's lock.
    private final class ActiveTurn: @unchecked Sendable {
        var collector = CodexTurnCollector()
        let outcome = CodexOneShot<CodexTurnCollector.Outcome>()
    }

    var isAlive: Bool { lock.withLock { launched && !exited } }

    init(config: CodexConfig) throws {
        process.executableURL = URL(fileURLWithPath: config.executable)
        process.arguments = CodexProtocol.arguments()
        process.currentDirectoryURL = config.workDirectory
        process.environment = CodexProtocol.environment(base: ProcessInfo.processInfo.environment,
                                                        home: FileManager.default.homeDirectoryForCurrentUser.path,
                                                        executable: config.executable)
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = Self.stderrLog(in: config.workDirectory) ?? FileHandle.nullDevice
        // A write after the child died must fail with EPIPE instead of killing the app with SIGPIPE.
        _ = fcntl(stdinPipe.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1)
        process.terminationHandler = { [weak self] p in
            // Reading terminationStatus is safe here: the process has exited.
            self?.didExit(status: p.terminationStatus)
        }
        try process.run()
        lock.withLock { launched = true }
        startReader()
        Log.formatter.notice("codex app-server pid \(self.process.processIdentifier, privacy: .public) started: \(config.executable, privacy: .public) cwd=\(config.workDirectory.path, privacy: .public)")
    }

    func handshake() async throws {
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
        _ = try await request("initialize", CodexProtocol.initializeParams(version: version))
        try write(["method": "initialized", "params": [String: Any]()])
    }

    /// Starts a thread and runs the warm-up turn, which opens the model connection and fills the prompt cache.
    func startWarmThread(_ config: CodexConfig) async throws -> String {
        let result = try await request("thread/start", CodexProtocol.threadStartParams(config))
        guard let threadID = CodexProtocol.threadID(fromStartResult: result.object) else {
            throw CodexError.rpc("thread/start returned no thread id")
        }
        _ = try await runTurn(threadID: threadID, text: CodexProtocol.warmUpMessage, effort: config.reasoningEffort)
        return threadID
    }

    /// Sends one user turn and waits for `turn/completed`. Returns the final agent message.
    func runTurn(threadID: String, text: String, effort: String?) async throws -> String {
        let turn = ActiveTurn()
        lock.withLock { turns[threadID] = turn }
        defer { lock.withLock { if turns[threadID] === turn { turns[threadID] = nil } } }
        let started = try await request("turn/start", CodexProtocol.turnStartParams(threadID: threadID, text: text, effort: effort))
        if let id = CodexProtocol.turnID(fromStartResult: started.object) {
            lock.withLock { if turn.collector.turnID == nil { turn.collector.turnID = id } }
        }
        switch try await turn.outcome.value() {
        case .reply(let text): return text
        case .failed(let reason): throw CodexError.turnFailed(reason)
        }
    }

    /// Lets Codex unload a thread Shepit no longer uses.
    func release(threadID: String) {
        Task { _ = try? await self.request("thread/unsubscribe", ["threadId": threadID]) }
    }

    func terminate() {
        try? stdinPipe.fileHandleForWriting.close()
        // Never touch a Process that did not launch: terminate() would raise an Objective-C exception.
        if lock.withLock({ launched }), process.isRunning { process.terminate() }
    }

    // MARK: connection

    private func request(_ method: String, _ params: [String: Any]) async throws -> CodexJSON {
        let waiter = CodexOneShot<CodexJSON>()
        let id: Int? = lock.withLock {
            guard launched, !exited else { return nil }
            let id = nextID
            nextID += 1
            pending[id] = waiter
            return id
        }
        guard let id else { throw CodexError.notRunning }
        do {
            try write(["method": method, "id": id, "params": params])
            return try await waiter.value()
        } catch {
            lock.withLock { pending[id] = nil }
            throw error
        }
    }

    private func write(_ message: [String: Any]) throws {
        let data = try CodexProtocol.encodeLine(message)
        try writeLock.withLock { try stdinPipe.fileHandleForWriting.write(contentsOf: data) }
    }

    private func startReader() {
        let output = stdoutPipe.fileHandleForReading
        // Blocking reads on a dedicated thread: `FileHandle.bytes` delivered nothing in the Finder-launched app.
        let reader = Thread { [weak self] in
            var buffer = Data()
            while true {
                let chunk = output.availableData
                if chunk.isEmpty { break }
                buffer.append(chunk)
                while let newline = buffer.firstIndex(of: 0x0A) {
                    let line = buffer.subdata(in: buffer.startIndex..<newline)
                    buffer.removeSubrange(buffer.startIndex...newline)
                    self?.receive(line)
                }
            }
            if !buffer.isEmpty { self?.receive(buffer) }
            self?.didCloseOutput()
        }
        reader.name = "shepit.codex.stdout"
        reader.qualityOfService = .userInitiated
        reader.start()
    }

    private func receive(_ line: Data) {
        guard !line.isEmpty, let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              let message = CodexProtocol.classify(object) else { return }
        switch message {
        case .response(let id, let result, let error):
            let waiter = lock.withLock { pending.removeValue(forKey: id) }
            if let error {
                waiter?.resolve(.failure(CodexError.rpc(error)))
            } else {
                waiter?.resolve(.success(CodexJSON(object: result)))
            }
        case .request(let id, let method, _):
            // With approvals set to never none should arrive; answer anyway so a turn never waits on Shepit.
            Log.formatter.notice("codex sent request \(method, privacy: .public); declining")
            try? write(CodexProtocol.reply(toRequest: method, id: id))
        case .notification(let method, let params):
            if method == "error" || method == "warning" || method == "configWarning" {
                // Params are server text and may echo the turn, so they stay private.
                Log.formatter.notice("codex \(method, privacy: .public): \(String(describing: params), privacy: .private)")
            }
            guard let threadID = params["threadId"] as? String else { return }
            let (turn, outcome): (ActiveTurn?, CodexTurnCollector.Outcome?) = lock.withLock {
                guard let turn = turns[threadID] else { return (nil, nil) }
                return (turn, turn.collector.consume(method: method, params: params))
            }
            if let turn, let outcome { turn.outcome.resolve(.success(outcome)) }
        }
    }

    private func didExit(status: Int32) {
        lock.withLock {
            exited = true
            exitStatus = status
        }
        Log.formatter.notice("codex app-server exited with status \(status, privacy: .public)")
        failAll()
    }

    private func didCloseOutput() {
        lock.withLock { exited = true }
        failAll()
    }

    private func failAll() {
        let (waiters, active, status) = lock.withLock { () -> ([CodexOneShot<CodexJSON>], [ActiveTurn], Int32) in
            let waiters = Array(pending.values)
            let active = Array(turns.values)
            pending.removeAll()
            turns.removeAll()
            return (waiters, active, exitStatus)
        }
        for waiter in waiters { waiter.resolve(.failure(CodexError.exited(status))) }
        for turn in active { turn.outcome.resolve(.failure(CodexError.exited(status))) }
    }

    private static func stderrLog(in directory: URL) -> FileHandle? {
        let url = directory.appendingPathComponent("codex-stderr.log")
        let fm = FileManager.default
        // Appended on every spawn; start over once it gets large.
        if let size = (try? fm.attributesOfItem(atPath: url.path))?[.size] as? Int, size > 1_000_000 {
            try? fm.removeItem(at: url)
        }
        if !fm.fileExists(atPath: url.path) { fm.createFile(atPath: url.path, contents: nil) }
        guard let handle = try? FileHandle(forWritingTo: url) else { return nil }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: Data("\n--- codex app-server spawned \(Date()) ---\n".utf8))
        return handle
    }
}

/// A result handed once from the reader thread to one waiting task. Cancelling the task resolves it.
private final class CodexOneShot<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var result: Result<Value, Error>?
    private var continuation: CheckedContinuation<Value, Error>?
    private var resolved = false

    func resolve(_ value: Result<Value, Error>) {
        let waiting: CheckedContinuation<Value, Error>? = lock.withLock {
            guard !resolved else { return nil }
            resolved = true
            if let continuation {
                self.continuation = nil
                return continuation
            }
            result = value
            return nil
        }
        waiting?.resume(with: value)
    }

    func value() async throws -> Value {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Value, Error>) in
                let ready: Result<Value, Error>? = lock.withLock {
                    if let result { return result }
                    self.continuation = continuation
                    return nil
                }
                if let ready { continuation.resume(with: ready) }
            }
        } onCancel: {
            resolve(.failure(CancellationError()))
        }
    }
}

/// Runs `operation` and throws `CodexError.timeout` if it takes longer than `seconds`; the operation is cancelled.
private func codexDeadline<T: Sendable>(_ seconds: TimeInterval,
                                        _ operation: @escaping @Sendable () async throws -> T) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask { try await operation() }
        group.addTask {
            try await Task.sleep(for: .seconds(seconds))
            throw CodexError.timeout
        }
        guard let first = try await group.next() else { throw CodexError.notRunning }
        group.cancelAll()
        return first
    }
}
