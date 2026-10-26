import Foundation

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
        do {
            return try await claude.format(request)
        } catch {
            Log.formatter.error("claude formatting failed, falling back: \(String(describing: error), privacy: .public)")
            return FormatResult(text: Self.local(request), language: request.languageHint,
                                source: .fallback, note: error.localizedDescription)
        }
    }

    /// Text for the paths without Claude: whitespace cleanup plus dictionary terms restored.
    private static func local(_ request: FormatRequest) -> String {
        PassthroughFormatter.clean(DictionaryCorrector.correct(request.transcript, dictionary: request.dictionary))
    }

    public func warmUp() async {
        await claude?.warmUp()
    }
}
