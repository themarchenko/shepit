import Foundation

/// Sends one calibration request to a polishing agent and returns its raw reply text.
public protocol CalibrationAgent: Sendable {
    func reply(to message: String, round: CalibrationRound) async throws -> String
}

public enum CalibrationError: Error, Equatable {
    /// Fewer than three readings: too little to describe a voice, so the agent is not called.
    case tooFewTakes
    case badReply
}

public enum CalibrationAgents {
    /// A new empty directory name per call, outside the formatter workers' directories, so a calibration run
    /// never shares files or logs with dictation. The agent creates it and removes it when the call ends.
    public static func freshWorkDirectory() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("shepit-calibration-\(UUID().uuidString)", isDirectory: true)
    }

    /// nil when polishing is off or the chosen agent's executable is unknown: calibration needs a working agent.
    public static func make(provider: PolishingProvider, claudeExecutable: String?, claudeModel: String,
                            codexExecutable: String?) -> CalibrationAgent? {
        switch provider {
        case .off:
            return nil
        case .claudeCode:
            return claudeExecutable.map { ClaudeCalibrationAgent(executable: $0, model: claudeModel) }
        case .codex:
            return codexExecutable.map { CodexCalibrationAgent(executable: $0) }
        }
    }
}

/// One `claude -p` process per call. Same arguments and safety flags as the formatter worker (no tools, no
/// settings, no MCP, no session file, no shell), a fresh work directory, and the process is gone when the call returns.
struct ClaudeCalibrationAgent: CalibrationAgent {
    let executable: String
    let model: String
    var timeout: TimeInterval = 90

    func reply(to message: String, round: CalibrationRound) async throws -> String {
        let directory = CalibrationAgents.freshWorkDirectory()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        var config = ClaudeWorkerConfig(executable: executable, model: model,
                                        systemPrompt: CalibrationMessages.systemPrompt, workDirectory: directory)
        config.effort = "medium"
        let worker: WorkerProcess
        do {
            worker = try WorkerProcess(config: config)
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
        defer {
            worker.terminate()
            try? FileManager.default.removeItem(at: directory)
        }
        do {
            return try await worker.send(message, timeout: timeout).text
        } catch {
            Log.formatter.error("calibration \(round.rawValue, privacy: .public) failed: \(CalibrationCoach.failureKind(error), privacy: .public)")
            throw error
        }
    }
}

/// One `codex app-server` process per call, with a fresh thread and the round's output schema.
struct CodexCalibrationAgent: CalibrationAgent {
    let executable: String
    var timeout: TimeInterval = 90

    func reply(to message: String, round: CalibrationRound) async throws -> String {
        let directory = CalibrationAgents.freshWorkDirectory()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        // An empty model uses whatever the user's Codex is set to, the same choice the formatter makes.
        let config = CodexConfig(executable: executable, model: "", reasoningEffort: "medium",
                                 systemPrompt: CalibrationMessages.systemPrompt, workDirectory: directory)
        let schema = SendableSchema(object: round == .targets ? CalibrationMessages.targetsSchema
                                                               : CalibrationMessages.profileSchema)
        let holder = ServerHolder()
        defer {
            holder.server?.terminate()
            try? FileManager.default.removeItem(at: directory)
        }
        do {
            return try await codexDeadline(timeout) {
                let server = try CodexAppServerProcess(config: config)
                holder.server = server
                try await server.handshake()
                let thread = try await server.startThread(config)
                return try await server.runTurn(threadID: thread, text: message, effort: "medium", schema: schema.object)
            }
        } catch {
            Log.formatter.error("calibration \(round.rawValue, privacy: .public) failed: \(CalibrationCoach.failureKind(error), privacy: .public)")
            throw error
        }
    }
}

private struct SendableSchema: @unchecked Sendable {
    let object: [String: Any]
}

/// Lets the cleanup run even when the deadline cancels the operation before it returns the server.
private final class ServerHolder: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: CodexAppServerProcess?
    var server: CodexAppServerProcess? {
        get { lock.withLock { stored } }
        set { lock.withLock { stored = newValue } }
    }
}

/// Turns pairs into prompts and a profile. The agent is optional: round 1 falls back to the fixed sentences.
public struct CalibrationCoach: Sendable {
    let agent: CalibrationAgent?
    let policy: LanguagePolicy

    public init(agent: CalibrationAgent?, policy: LanguagePolicy) {
        self.agent = agent
        self.policy = policy
    }

    /// Error kind for logs: the case name only, never reply or server text.
    public static func failureKind(_ error: Error) -> String {
        if let error = error as? CalibrationError {
            return error == .tooFewTakes ? "tooFewTakes" : "badReply"
        }
        return FormattingService.logKind(error)
    }

    /// Sentences aimed at what went wrong, or the fixed ones. `usedAgent` says which, so the UI can word its subtitle.
    public func targets(pairs: [CalibrationPair]) async -> ([CalibrationPrompt], usedAgent: Bool) {
        let fallback = CalibrationScript.fallback(for: policy)
        guard let agent, !pairs.isEmpty else { return (fallback, false) }
        do {
            let reply = try await agent.reply(to: CalibrationMessages.request(round: .targets, policy: policy, pairs: pairs),
                                              round: .targets)
            if let prompts = CalibrationMessages.parseTargets(reply, policy: policy) { return (prompts, true) }
            Log.formatter.error("calibration targets failed: badReply")
        } catch {
            Log.formatter.error("calibration targets failed: \(Self.failureKind(error), privacy: .public)")
        }
        return (fallback, false)
    }

    public func profile(pairs: [CalibrationPair], existingDictionary: [String]) async throws -> CalibrationProfile {
        guard pairs.count >= 3, let agent else { throw CalibrationError.tooFewTakes }
        let reply = try await agent.reply(to: CalibrationMessages.request(round: .profile, policy: policy, pairs: pairs),
                                          round: .profile)
        guard let profile = CalibrationMessages.parseProfile(reply, existingDictionary: existingDictionary) else {
            Log.formatter.error("calibration profile failed: badReply")
            throw CalibrationError.badReply
        }
        return profile
    }
}
