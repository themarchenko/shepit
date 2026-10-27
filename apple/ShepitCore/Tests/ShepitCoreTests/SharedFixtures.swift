import Foundation

/// Loader for the language-neutral fixtures in `shared/fixtures`. The Rust port reads the same files.
struct FixtureCase<I: Decodable, E: Decodable>: Decodable {
    let name: String
    let input: I
    let expected: E

    private enum CodingKeys: String, CodingKey { case name, input, expected }
}

struct FixtureFile<I: Decodable, E: Decodable>: Decodable {
    let version: Int
    let description: String
    let cases: [FixtureCase<I, E>]

    private enum CodingKeys: String, CodingKey { case version, description, cases }
}

struct FixtureError: Error, CustomStringConvertible {
    let description: String
}

enum SharedFixtures {
    /// Repository's `apple/` parent: file, ShepitCoreTests, Tests, ShepitCore, apple. If this is off by one,
    /// every runner would find nothing, so the loader also fails on a missing file and on too few cases.
    static let root: URL = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()

    static var fixturesDirectory: URL {
        root.appendingPathComponent("shared").appendingPathComponent("fixtures")
    }

    static let fileNames: [String] = [
        "dictionary-correction.json",
        "dictionary-learning.json",
        "hotkey-gestures.json",
        "passthrough-clean.json",
        "word-count.json",
        "profile-resolution.json",
        "mode-resolution.json",
        "hallucination-filter.json",
        "retry-language.json",
        "live-preview-join.json",
        "live-preview-split.json",
        "app-version.json",
        "prompt-message.json",
        "reply-parse.json",
        "reply-finish.json",
        "polish-routing.json",
    ]

    static func load<I: Decodable, E: Decodable>(_ fileName: String, minimumCases: Int) throws -> FixtureFile<I, E> {
        guard minimumCases >= 1 else {
            throw FixtureError(description: "minimumCases must be at least 1 for \(fileName)")
        }
        let url = fixturesDirectory.appendingPathComponent(fileName)
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw FixtureError(description: "missing fixture \(url.path)")
        }
        let file = try JSONDecoder().decode(FixtureFile<I, E>.self, from: Data(contentsOf: url))
        guard file.version == 1 else {
            throw FixtureError(description: "\(fileName): unsupported version \(file.version)")
        }
        guard file.cases.count >= minimumCases else {
            throw FixtureError(description: "\(fileName): \(file.cases.count) cases, expected at least \(minimumCases)")
        }
        return file
    }
}

// Input and expected shapes, one per file. Keys are snake_case in JSON, spelled out on purpose.

struct DictionaryCorrectionInput: Decodable {
    let text: String
    let dictionary: [String]
    private enum CodingKeys: String, CodingKey { case text, dictionary }
}

struct DictionaryLearningInput: Decodable {
    let reported: [String]
    let existing: [String]
    private enum CodingKeys: String, CodingKey { case reported, existing }
}

struct HotkeyEvent: Decodable {
    let key: String
    let at: Double
    private enum CodingKeys: String, CodingKey { case key, at }
}

struct HotkeyInput: Decodable {
    let events: [HotkeyEvent]
    private enum CodingKeys: String, CodingKey { case events }
}

struct HotkeyOutput: Decodable {
    let output: String
    let duration: Double?
    private enum CodingKeys: String, CodingKey { case output, duration }
}

struct TextInput: Decodable {
    let text: String
    private enum CodingKeys: String, CodingKey { case text }
}

struct ProfileInput: Decodable {
    let bundleID: String?
    let windowTitle: String?
    let url: String?
    private enum CodingKeys: String, CodingKey {
        case bundleID = "bundle_id"
        case windowTitle = "window_title"
        case url
    }
}

struct ModeInput: Decodable {
    let requested: String
    let profile: String
    let wordCount: Int
    let promptModeMinWords: Int
    private enum CodingKeys: String, CodingKey {
        case requested, profile
        case wordCount = "word_count"
        case promptModeMinWords = "prompt_mode_min_words"
    }
}

struct RetryLanguageInput: Decodable {
    let detected: String?
    let text: String
    private enum CodingKeys: String, CodingKey { case detected, text }
}

struct JoinInput: Decodable {
    let a: String
    let b: String
    private enum CodingKeys: String, CodingKey { case a, b }
}

struct SplitInput: Decodable {
    let frozen: String
    let current: String
    private enum CodingKeys: String, CodingKey { case frozen, current }
}

struct SplitExpected: Decodable {
    let stable: String
    let unstable: String
    private enum CodingKeys: String, CodingKey { case stable, unstable }
}

struct VersionInput: Decodable {
    let candidate: String
    let installed: String
    private enum CodingKeys: String, CodingKey { case candidate, installed }
}

struct PromptContextInput: Decodable {
    let app: String?
    let bundleID: String?
    let windowTitle: String?
    let url: String?
    let textBeforeCaret: String?
    let selectedText: String?
    private enum CodingKeys: String, CodingKey {
        case app, url
        case bundleID = "bundle_id"
        case windowTitle = "window_title"
        case textBeforeCaret = "text_before_caret"
        case selectedText = "selected_text"
    }
}

struct ExampleInput: Decodable {
    let raw: String
    let final: String
    private enum CodingKeys: String, CodingKey { case raw, final }
}

struct PromptMessageInput: Decodable {
    let transcript: String
    let languageHint: String?
    let mode: String
    let profile: String
    let context: PromptContextInput
    let dictionary: [String]?
    let examples: [ExampleInput]?
    private enum CodingKeys: String, CodingKey {
        case transcript, mode, profile, context, dictionary, examples
        case languageHint = "language_hint"
    }
}

struct ReplyInput: Decodable {
    let reply: String
    private enum CodingKeys: String, CodingKey { case reply }
}

struct ParsedReplyExpected: Decodable, Equatable {
    let text: String
    let language: String?
    let newTerms: [String]
    private enum CodingKeys: String, CodingKey {
        case text, language
        case newTerms = "new_terms"
    }
}

struct FinishInput: Decodable {
    let text: String
    let profile: String
    private enum CodingKeys: String, CodingKey { case text, profile }
}

struct RoutingInput: Decodable {
    let mode: String
    let profile: String
    let transcript: String
    let fastPathMaxWords: Int
    let dictionary: [String]
    let agent: String
    let agentText: String?
    /// Missing means false.
    let secure: Bool?
    private enum CodingKeys: String, CodingKey {
        case mode, profile, transcript, dictionary, agent, secure
        case fastPathMaxWords = "fast_path_max_words"
        case agentText = "agent_text"
    }
}

struct RoutingExpected: Decodable, Equatable {
    let source: String
    let text: String
    private enum CodingKeys: String, CodingKey { case source, text }
}
