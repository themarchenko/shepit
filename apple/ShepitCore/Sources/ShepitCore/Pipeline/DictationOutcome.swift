import Foundation

/// How a dictation ended once the text was ready.
public enum DictationOutcome: Equatable, Sendable {
    case inserted(InsertMethod)
    /// Review ended without pasting (Esc, empty text, quit).
    case discarded
    /// Review was confirmed but the insert was refused.
    case failed
}

/// The history columns that depend on the outcome.
public struct OutcomeFields: Equatable, Sendable {
    public let finalText: String?
    public let insertMethod: String
    public let status: String

    public init(formattedText: String, sentText: String, outcome: DictationOutcome) {
        // Exact compare: a trailing space the user typed is an edit.
        let edited: String? = sentText != formattedText ? sentText : nil
        switch outcome {
        case .inserted(let method):
            finalText = edited
            insertMethod = method.rawValue
            status = method == .clipboardOnly ? "clipboard" : "inserted"
        case .discarded:
            // finalText records what was pasted, and nothing was.
            finalText = nil
            insertMethod = "none"
            status = "discarded"
        case .failed:
            // Keep the edit so it can be copied back out of History.
            finalText = edited
            insertMethod = "none"
            status = "failed"
        }
    }
}

public enum ReviewPolicy {
    public static func shouldReview(setting: Bool, shiftHeld: Bool) -> Bool { setting || shiftHeld }
}

/// What the last finished insert looked like, for the one-time review tip.
public struct InsertSummary: Equatable, Sendable {
    public let reviewed: Bool
    public let method: InsertMethod
    public init(reviewed: Bool, method: InsertMethod) {
        self.reviewed = reviewed
        self.method = method
    }
}

public enum ReviewTip {
    /// Clipboard-only inserts are skipped: the user has a permission problem to fix first.
    public static func shouldShow(tipShown: Bool, reviewBeforePaste: Bool, onboardingCompleted: Bool,
                                  insert: InsertSummary) -> Bool {
        !tipShown && !reviewBeforePaste && onboardingCompleted && !insert.reviewed && insert.method != .clipboardOnly
    }
}
