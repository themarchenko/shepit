import Foundation

public struct FormatRequest: Sendable {
    public var transcript: String
    public var languageHint: String?
    public var mode: Mode
    public var profile: Profile
    public var context: TargetContext
    public var dictionary: [String]
    /// (raw transcript, accepted final text) pairs for this profile.
    public var examples: [(String, String)]

    public init(transcript: String, languageHint: String?, mode: Mode, profile: Profile, context: TargetContext,
                dictionary: [String] = [], examples: [(String, String)] = []) {
        self.transcript = transcript
        self.languageHint = languageHint
        self.mode = mode
        self.profile = profile
        self.context = context
        self.dictionary = dictionary
        self.examples = examples
    }
}

public enum FormatterSource: String, Codable, Sendable {
    case claude
    case codex
    case passthrough
    case fastPath
    case fallback
}

public struct FormatResult: Sendable {
    public var text: String
    public var language: String?
    public var newTerms: [String]
    public var source: FormatterSource
    public var latency: TimeInterval
    public var note: String?

    public init(text: String, language: String? = nil, newTerms: [String] = [], source: FormatterSource, latency: TimeInterval = 0, note: String? = nil) {
        self.text = text
        self.language = language
        self.newTerms = newTerms
        self.source = source
        self.latency = latency
        self.note = note
    }
}

public protocol Formatter: Sendable {
    func format(_ request: FormatRequest) async throws -> FormatResult
    func warmUp() async
}

/// Returns the recognizer text with whitespace cleanup only.
public struct PassthroughFormatter: Formatter {
    public init() {}

    public func format(_ request: FormatRequest) async throws -> FormatResult {
        FormatResult(text: Self.clean(request.transcript), language: request.languageHint, source: .passthrough)
    }

    public func warmUp() async {}

    public static func clean(_ text: String) -> String {
        let collapsed = text.split(whereSeparator: { $0.isWhitespace || $0.isNewline }).joined(separator: " ")
        guard let first = collapsed.first else { return "" }
        return first.uppercased() + collapsed.dropFirst()
    }
}

public enum JSONExtraction {
    /// Finds the first JSON object in a model reply that may be wrapped in prose or code fences.
    public static func firstObject(in text: String) -> [String: Any]? {
        var s = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.hasPrefix("```") {
            s = s.replacingOccurrences(of: "```json", with: "").replacingOccurrences(of: "```", with: "")
        }
        guard let start = s.firstIndex(of: "{"), let end = s.lastIndex(of: "}"), start < end else { return nil }
        let candidate = String(s[start...end])
        if let data = candidate.data(using: .utf8),
           let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            return obj
        }
        // Model replies sometimes contain raw newlines inside strings; escape them and retry.
        let escaped = candidate.replacingOccurrences(of: "\n", with: "\\n").replacingOccurrences(of: "\t", with: "\\t")
        if let data = escaped.data(using: .utf8),
           let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            return obj
        }
        return nil
    }
}
