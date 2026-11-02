import Foundation

public enum AgentTestResult: Equatable, Sendable {
    case passed
    case noAgent
    case failed(kind: String)
}

/// Picks the path for one dictation: raw, fast path, Claude, or passthrough when Claude fails.
public actor FormattingService {
    private var claude: Formatter?
    private let passthrough = PassthroughFormatter()
    public var fastPathMaxWords = 8

    public init(claude: Formatter?) {
        self.claude = claude
    }

    public func setClaude(_ formatter: Formatter?) {
        claude = formatter
    }

    public func setFastPath(maxWords: Int) {
        fastPathMaxWords = max(0, maxWords)
    }

    public func format(_ request: FormatRequest) async -> FormatResult {
        if request.mode == .raw {
            return FormatResult(text: PassthroughFormatter.clean(request.transcript), language: request.languageHint, source: .passthrough)
        }
        let words = ModeResolver.wordCount(request.transcript)
        if request.mode == .clean, !request.profile.isAITarget, words <= fastPathMaxWords {
            return FormatResult(text: Self.local(request), language: request.languageHint, source: .fastPath)
        }
        guard let claude else {
            return FormatResult(text: Self.local(request), language: request.languageHint, source: .passthrough)
        }
        // The transcript may be the password itself; do not send it to the agent for a field the insert will refuse.
        if request.context.secureInput {
            return FormatResult(text: Self.local(request), language: request.languageHint, source: .passthrough)
        }
        do {
            return try await claude.format(request)
        } catch {
            Log.formatter.error("claude formatting failed, falling back: \(Self.logKind(error), privacy: .public)")
            return FormatResult(text: Self.local(request), language: request.languageHint,
                                source: .fallback, note: error.localizedDescription)
        }
    }

    /// Error kind for logs: the case name and byte counts, never reply or server text.
    static func logKind(_ error: Error) -> String {
        switch error {
        case let e as WorkerError:
            switch e {
            case .notRunning: return "notRunning"
            case .timeout: return "timeout"
            case .exited(let code): return "exited(\(code))"
            case .apiError: return "apiError"
            case .badReply(let n): return "badReply(\(n) bytes)"
            }
        case let e as CodexError:
            switch e {
            case .notRunning: return "notRunning"
            case .timeout: return "timeout"
            case .exited(let code): return "exited(\(code))"
            case .rpc: return "rpc"
            case .turnFailed: return "turnFailed"
            case .badReply(let n): return "badReply(\(n) bytes)"
            case .paused: return "paused"
            }
        default:
            return String(describing: type(of: error))
        }
    }

    /// Text for the paths without Claude: whitespace cleanup plus dictionary terms restored.
    private static func local(_ request: FormatRequest) -> String {
        PassthroughFormatter.clean(DictionaryCorrector.correct(request.transcript, dictionary: request.dictionary))
    }

    public static let doctorSample = "This is a short test from Shepit Doctor."

    /// Sends the fixed sample straight to the agent, skipping the fast path, with no user context.
    public func testAgent() async -> AgentTestResult {
        guard let claude else { return .noAgent }
        let request = FormatRequest(transcript: Self.doctorSample, languageHint: "en", mode: .clean,
                                    profile: BuiltInProfiles.generic, context: TargetContext())
        do {
            _ = try await claude.format(request)
            return .passed
        } catch {
            return .failed(kind: Self.logKind(error))
        }
    }

    public func warmUp() async {
        await claude?.warmUp()
    }
}
