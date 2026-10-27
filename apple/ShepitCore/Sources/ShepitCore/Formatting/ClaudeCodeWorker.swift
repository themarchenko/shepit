import Foundation

public struct ClaudeWorkerConfig: Sendable {
    public var executable: String
    public var model: String
    /// Claude Code effort level; formatting is routine work, so low keeps latency down.
    public var effort: String = "low"
    public var systemPrompt: String
    public var workDirectory: URL
    public var maxTurns: Int = 25
    public var standbyAfterTurns: Int = 20
    public var requestTimeout: TimeInterval = 25
    public var warmUpTimeout: TimeInterval = 40

    public init(executable: String, model: String, systemPrompt: String, workDirectory: URL) {
        self.executable = executable
        self.model = model
        self.systemPrompt = systemPrompt
        self.workDirectory = workDirectory
    }
}

public enum WorkerError: Error, LocalizedError {
    case notRunning
    case timeout
    case exited(Int32)
    case apiError(String)
    case badReply(bytes: Int)

    public var errorDescription: String? {
        switch self {
        case .notRunning: return "Claude worker is not running"
        case .timeout: return "Claude worker timed out"
        case .exited(let code): return "Claude worker exited with code \(code)"
        case .apiError(let s): return "Claude error: \(s)"
        case .badReply(let n): return "Claude reply was not JSON (\(n) bytes)"
        }
    }
}

/// One `claude -p --input-format stream-json` process. One request in flight at a time.
final class WorkerProcess: @unchecked Sendable {
    let process = Process()
    private let stdin = Pipe()
    private let stdout = Pipe()
    private let stderr = Pipe()
    private var events: AsyncStream<[String: Any]>
    private var iterator: AsyncStream<[String: Any]>.Iterator
    private(set) var turns = 0
    private(set) var lastUsed = Date()
    private(set) var isAlive = false
    let startedAt = Date()

    init(config: ClaudeWorkerConfig) throws {
        let promptURL = config.workDirectory.appendingPathComponent("system-prompt.md")
        try config.systemPrompt.write(to: promptURL, atomically: true, encoding: .utf8)
        // Claude Code appends to its debug file; keep only the latest worker's log.
        let debugURL = config.workDirectory.appendingPathComponent("worker-debug.log")
        try? FileManager.default.removeItem(at: debugURL)

        process.executableURL = URL(fileURLWithPath: config.executable)
        process.arguments = [
            "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
            "--model", config.model, "--effort", config.effort, "--tools", "", "--setting-sources", "", "--strict-mcp-config",
            "--no-session-persistence", "--system-prompt-file", promptURL.path,
            "--debug-file", debugURL.path,
        ]
        process.currentDirectoryURL = config.workDirectory
        var env = ProcessInfo.processInfo.environment
        for key in ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SSE_PORT"] { env.removeValue(forKey: key) }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        env["PATH"] = "\(home)/.local/bin:/opt/homebrew/bin:/usr/local/bin:" + (env["PATH"] ?? "/usr/bin:/bin")
        env["HOME"] = home
        process.environment = env
        process.standardInput = stdin
        process.standardOutput = stdout
        let stderrURL = config.workDirectory.appendingPathComponent("worker-stderr.log")
        if !FileManager.default.fileExists(atPath: stderrURL.path) {
            FileManager.default.createFile(atPath: stderrURL.path, contents: nil)
        }
        if let handle = try? FileHandle(forWritingTo: stderrURL) {
            try? handle.seekToEnd()
            try? handle.write(contentsOf: Data("\n--- worker spawned \(Date()) ---\n".utf8))
            process.standardError = handle
        } else {
            process.standardError = stderr
        }

        var continuation: AsyncStream<[String: Any]>.Continuation!
        events = AsyncStream { continuation = $0 }
        iterator = events.makeAsyncIterator()
        let out = stdout.fileHandleForReading
        let cont = continuation!
        // A dedicated thread with blocking reads: independent of run loops and of the cooperative pool.
        let reader = Thread {
            var buffer = Data()
            while true {
                let chunk = out.availableData
                if chunk.isEmpty { break }
                buffer.append(chunk)
                while let newline = buffer.firstIndex(of: 0x0A) {
                    let lineData = buffer.subdata(in: buffer.startIndex..<newline)
                    buffer.removeSubrange(buffer.startIndex...newline)
                    guard !lineData.isEmpty,
                          let obj = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any] else { continue }
                    cont.yield(obj)
                }
            }
            if !buffer.isEmpty, let obj = try? JSONSerialization.jsonObject(with: buffer) as? [String: Any] {
                cont.yield(obj)
            }
            cont.finish()
        }
        reader.name = "shepit.claude-worker.stdout"
        reader.qualityOfService = .userInitiated
        reader.start()
        try process.run()
        isAlive = true
        Log.formatter.notice("worker pid \(self.process.processIdentifier, privacy: .public) started: \(config.executable, privacy: .public) model=\(config.model, privacy: .public) cwd=\(config.workDirectory.path, privacy: .public)")
        process.terminationHandler = { [weak self] p in
            self?.isAlive = false
            Log.formatter.notice("worker exited with status \(p.terminationStatus, privacy: .public)")
        }
    }

    struct Reply: Sendable {
        let text: String
        let isError: Bool
        let subtype: String?
        let usage: [String: Int]
    }

    /// Sends one user turn and waits for its `result` event. Returns the reply text.
    func send(_ content: String, timeout: TimeInterval) async throws -> (text: String, usage: [String: Int]) {
        guard isAlive else { throw WorkerError.notRunning }
        let message: [String: Any] = ["type": "user", "message": ["role": "user", "content": content]]
        var data = try JSONSerialization.data(withJSONObject: message)
        data.append(0x0A)
        try stdin.fileHandleForWriting.write(contentsOf: data)
        lastUsed = Date()

        let reply: Reply = try await withThrowingTaskGroup(of: Reply.self) { group in
            group.addTask { [self] in
                while let event = await self.iterator.next() {
                    guard event["type"] as? String == "result" else { continue }
                    let usage = (event["usage"] as? [String: Any] ?? [:]).compactMapValues { $0 as? Int }
                    return Reply(text: (event["result"] as? String) ?? "",
                                 isError: event["is_error"] as? Bool ?? false,
                                 subtype: event["subtype"] as? String,
                                 usage: usage)
                }
                // The stream ends when the task is cancelled (timeout) or when the process closed stdout.
                if Task.isCancelled { throw CancellationError() }
                throw WorkerError.exited(self.process.isRunning ? -1 : self.process.terminationStatus)
            }
            group.addTask {
                try await Task.sleep(for: .seconds(timeout))
                throw WorkerError.timeout
            }
            guard let first = try await group.next() else { throw WorkerError.notRunning }
            group.cancelAll()
            return first
        }
        turns += 1
        lastUsed = Date()
        if reply.isError {
            throw WorkerError.apiError(reply.text.isEmpty ? (reply.subtype ?? "unknown") : reply.text)
        }
        return (reply.text, reply.usage)
    }

    func terminate() {
        guard isAlive else { return }
        try? stdin.fileHandleForWriting.close()
        process.terminate()
    }
}

/// Persistent headless Claude Code session used as the formatter. Recycles the process so the conversation never grows unbounded.
public actor ClaudeCodeWorker: Formatter {
    private let config: ClaudeWorkerConfig
    private var active: WorkerProcess?
    private var standby: WorkerProcess?
    private var standbyTask: Task<Void, Never>?
    private var busy = false
    public private(set) var lastUsage: [String: Int] = [:]

    public init(config: ClaudeWorkerConfig) {
        self.config = config
    }

    public func warmUp() async {
        do {
            _ = try await ensureActive()
        } catch {
            Log.formatter.error("warm-up failed: \(FormattingService.logKind(error), privacy: .public)")
        }
    }

    public func format(_ request: FormatRequest) async throws -> FormatResult {
        while busy { try await Task.sleep(for: .milliseconds(50)) }
        busy = true
        defer { busy = false }
        let proc = try await ensureActive()
        let t0 = Date()
        let reply: (text: String, usage: [String: Int])
        do {
            reply = try await proc.send(PromptBuilder.userMessage(request), timeout: config.requestTimeout)
        } catch {
            // The process is in an unknown state after a timeout or an exit; drop it.
            proc.terminate()
            if active === proc { active = nil }
            throw error
        }
        lastUsage = reply.usage
        rotateIfNeeded()
        guard let parsed = PromptBuilder.parseReply(reply.text) else { throw WorkerError.badReply(bytes: reply.text.utf8.count) }
        var text = parsed.text.trimmingCharacters(in: .newlines)
        if request.profile.stripTrailingNewline {
            while text.hasSuffix("\n") { text.removeLast() }
        }
        // Haiku sometimes keeps the lowercase start of a transcript whose first word was a filler.
        if let first = text.first, first.isLetter, first.isLowercase {
            text = first.uppercased() + text.dropFirst()
        }
        return FormatResult(text: text, language: parsed.language, newTerms: parsed.newTerms, source: .claude,
                            latency: Date().timeIntervalSince(t0))
    }

    public func shutdown() {
        active?.terminate()
        standby?.terminate()
        standbyTask?.cancel()
        active = nil
        standby = nil
    }

    // MARK: lifecycle

    private var consecutiveFailures = 0
    private var pausedUntil: Date?

    private func ensureActive() async throws -> WorkerProcess {
        if let active, active.isAlive { return active }
        if let standby, standby.isAlive {
            active = standby
            self.standby = nil
            return standby
        }
        if let pausedUntil, pausedUntil > Date() {
            throw WorkerError.apiError("worker paused after repeated failures until \(pausedUntil.formatted(date: .omitted, time: .standard))")
        }
        let proc = try spawn()
        do {
            try await warm(proc)
        } catch {
            proc.terminate()
            consecutiveFailures += 1
            if consecutiveFailures >= 2 {
                pausedUntil = Date().addingTimeInterval(300)
                Log.formatter.error("worker failed \(self.consecutiveFailures, privacy: .public) times in a row; pausing Claude formatting for 5 minutes")
            }
            throw error
        }
        consecutiveFailures = 0
        pausedUntil = nil
        active = proc
        return proc
    }

    private func spawn() throws -> WorkerProcess {
        Log.formatter.notice("spawning claude worker (\(self.config.model, privacy: .public))")
        return try WorkerProcess(config: config)
    }

    /// First turn creates the prompt cache; doing it here keeps the first real dictation fast.
    private func warm(_ proc: WorkerProcess) async throws {
        let t0 = Date()
        do {
            _ = try await proc.send("Warm-up. Return exactly: {\"text\": \"ok\", \"language\": \"en\", \"new_terms\": []}", timeout: config.warmUpTimeout)
        } catch {
            Log.formatter.error("worker warm-up failed after \(Date().timeIntervalSince(t0), format: .fixed(precision: 1)) s: \(FormattingService.logKind(error), privacy: .public); see \(self.config.workDirectory.path, privacy: .public)/worker-stderr.log")
            throw error
        }
        Log.formatter.notice("worker warm in \(Date().timeIntervalSince(t0), format: .fixed(precision: 2)) s")
    }

    private func rotateIfNeeded() {
        guard let active else { return }
        if active.turns >= config.standbyAfterTurns, standby == nil, standbyTask == nil {
            standbyTask = Task { [weak self] in
                guard let self else { return }
                await self.prepareStandby()
            }
        }
        if active.turns >= config.maxTurns, let standby, standby.isAlive {
            active.terminate()
            self.active = standby
            self.standby = nil
            Log.formatter.notice("rotated to standby worker")
        }
    }

    private func prepareStandby() async {
        defer { standbyTask = nil }
        do {
            let proc = try spawn()
            try await warm(proc)
            standby = proc
        } catch {
            Log.formatter.error("standby spawn failed: \(FormattingService.logKind(error), privacy: .public)")
        }
    }
}
