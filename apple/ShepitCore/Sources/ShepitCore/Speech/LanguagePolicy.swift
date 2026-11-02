import Foundation

/// Which languages a dictation may come out in. Russian and Polish are never allowed: the speaker uses
/// Ukrainian and English only, and recognizers that know more languages leak the others into the text.
/// The raw value is what `AppSettings` stores under `languageLock`.
public enum LanguagePolicy: String, CaseIterable, Sendable {
    case ukrainianEnglish = "uk+en"
    case ukrainian = "uk"
    case english = "en"

    /// Anything unknown, including the old "auto", becomes the default; the stored value is not rewritten.
    public init(stored: String?) {
        self = stored.flatMap(LanguagePolicy.init(rawValue:)) ?? .ukrainianEnglish
    }

    /// Ukrainian comes first: it wins ties.
    public var allowed: [String] {
        switch self {
        case .ukrainianEnglish: return ["uk", "en"]
        case .ukrainian: return ["uk"]
        case .english: return ["en"]
        }
    }

    /// The language to lock recognizers to, nil when both are allowed and the engine detects per utterance.
    public var hintLanguage: String? {
        allowed.count == 1 ? allowed[0] : nil
    }

    public func isAllowed(_ code: String?) -> Bool {
        guard let code else { return false }
        return allowed.contains(code)
    }

    /// The allowed language with the highest probability, nil when none of the allowed codes is present.
    public func pick(fromProbabilities probabilities: [String: Float]) -> String? {
        if allowed.count == 1 { return allowed[0] }
        var best: (code: String, p: Float)?
        for code in allowed {
            guard let p = probabilities[code] else { continue }
            if best == nil || p > best!.p { best = (code, p) }
        }
        return best?.code
    }

    /// The language hint for the polishing agent: the lock when there is one, else what recognition reported.
    public func formatterHint(reported: String?) -> String? {
        hintLanguage ?? reported
    }

    public var displayName: String {
        switch self {
        case .ukrainianEnglish: return L("Ukrainian + English")
        case .ukrainian: return L("Ukrainian")
        case .english: return L("English")
        }
    }
}

extension TranscriptionHints {
    /// A hint other than "uk" or "en" never comes from the settings; treat it as both allowed.
    public var policy: LanguagePolicy {
        switch language {
        case "uk": return .ukrainian
        case "en": return .english
        default: return .ukrainianEnglish
        }
    }
}
