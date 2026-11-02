import Foundation

/// Fixes for letters the recognizers write that belong to a language the speaker does not use.
public enum ScriptCleanup {
    /// Parakeet's Cyrillic model also knows Russian, and in a Ukrainian dictation it sometimes writes the
    /// Russian-only letters. Ukrainian has no ы, э, ъ or ё, so each maps to its nearest Ukrainian spelling.
    nonisolated public static func ukrainianLetters(_ text: String) -> String {
        let map: [Character: Character] = ["ы": "и", "э": "е", "ъ": "\u{02BC}", "ё": "е",
                                           "Ы": "И", "Э": "Е", "Ъ": "\u{02BC}", "Ё": "Е"]
        return String(text.map { map[$0] ?? $0 })
    }

    /// Letters Polish has and neither Ukrainian nor English does. "ó" is left out: it appears in loanwords.
    public static func hasPolishLetters(_ text: String) -> Bool {
        text.contains { "ąćęłńśźżĄĆĘŁŃŚŹŻ".contains($0) }
    }

    /// The map applies to the final text of every engine; only the text changes.
    public static func finalTranscript(_ t: Transcript) -> Transcript {
        Transcript(text: ukrainianLetters(t.text), language: t.language, audioSeconds: t.audioSeconds,
                   processingSeconds: t.processingSeconds, confidence: t.confidence)
    }

    /// Parakeet with no language hint sometimes writes Ukrainian speech as Polish-looking Latin. When the first
    /// pass has Polish letters and both languages are allowed, decode again as each allowed language and keep
    /// the most confident of the three (a missing confidence counts as 0, a tie keeps the earlier candidate,
    /// the first pass first). If every re-decode fails the first pass stays.
    public static func resolvePolishLeak(first: Transcript, policy: LanguagePolicy,
                                         decode: (String) async throws -> Transcript) async -> Transcript {
        guard policy.allowed.count > 1, hasPolishLetters(first.text) else { return first }
        // The first pass competes too: one Polish name in English text must not be replaced by a worse decode.
        var best: (t: Transcript, c: Float)? = (first, first.confidence ?? 0)
        for code in policy.allowed {
            guard let t = try? await decode(code) else { continue }
            let c = t.confidence ?? 0
            if best == nil || c > best!.c { best = (t, c) }
        }
        return best!.t
    }
}
