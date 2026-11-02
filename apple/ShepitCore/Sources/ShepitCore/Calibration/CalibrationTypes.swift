import Foundation

/// What the recognizer produced for one calibration reading. `raw` is the engine output, `cleaned` is the same
/// text after the Russian-letter map and the junk filter. Kept apart so the agent can see what the engine did.
public struct CalibrationTake: Sendable, Equatable {
    public var raw: String
    public var cleaned: String
    public var language: String?

    public init(raw: String, cleaned: String, language: String?) {
        self.raw = raw
        self.cleaned = cleaned
        self.language = language
    }
}

/// A reference text and what was heard for it. `raw` equals `heard` when cleaning changed nothing.
public struct CalibrationPair: Sendable, Equatable {
    public var reference: String
    public var heard: String
    public var raw: String

    public init(reference: String, heard: String, raw: String) {
        self.reference = reference
        self.heard = heard
        self.raw = raw
    }
}

public enum CalibrationRound: String, Sendable {
    case targets
    case profile
}

/// The reply of round 2: notes for the polishing agent and dictionary terms to offer.
public struct CalibrationProfile: Sendable, Equatable {
    public var notes: String
    public var terms: [String]

    public init(notes: String, terms: [String]) {
        self.notes = notes
        self.terms = terms
    }
}
