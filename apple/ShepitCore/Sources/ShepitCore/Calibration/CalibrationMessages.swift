import Foundation

/// The prompt, request and reply handling for the two one-off agent rounds. Pure, so the filters are tested
/// without an agent.
public enum CalibrationMessages {
    public static let systemPrompt = """
    You help calibrate Shepit, a dictation app. The speaker read reference texts aloud and an on-device recognizer wrote what it heard. You get JSON: round, languages (uk = Ukrainian, en = English), takes [{reference, heard, raw?}]. raw is the recognizer output before Shepit replaced Russian-only letters, present only when it differs. The speaker never speaks Russian or Polish: Russian or Polish words or letters are recognizer errors. An English technical term written in Cyrillic is normal transliteration, not a speech problem. An empty heard means nothing was recognized.
    Round "targets": find the sounds and words that went wrong (dropped, weak or swapped sounds, words that came out Russian or Polish). Write 2 or 3 new sentences of 6 to 14 words, only in the listed languages, that repeat those sounds and words. Ukrainian sentences may contain English technical terms. Return only {"sentences": [string]}.
    Round "profile": write notes for the agent that polishes this speaker's dictations. Plain English, at most 8 short lines, no headings or markdown. Say which languages the speaker uses and that Russian and Polish are never meant. Name the sounds the speaker swallows, weakens or swaps, each with how the recognizer writes it and what was meant. List words the recognizer gets wrong with the correct form. Describe only what the takes show. Also give up to 12 dictionary terms: words or names from the references the recognizer got wrong, correctly spelled. Return only {"notes": string, "terms": [string]}.
    No text outside the JSON.
    """

    public static let targetsSchema: [String: Any] = [
        "type": "object",
        "properties": ["sentences": ["type": "array", "items": ["type": "string"]]],
        "required": ["sentences"],
        "additionalProperties": false,
    ]

    public static let profileSchema: [String: Any] = [
        "type": "object",
        "properties": [
            "notes": ["type": "string"],
            "terms": ["type": "array", "items": ["type": "string"]],
        ],
        "required": ["notes", "terms"],
        "additionalProperties": false,
    ]

    /// Only language codes and the reference/heard texts. No audio, history, dictionary or context.
    public static func request(round: CalibrationRound, policy: LanguagePolicy, pairs: [CalibrationPair]) -> String {
        let takes: [[String: String]] = pairs.map { pair in
            var take = ["reference": pair.reference, "heard": pair.heard]
            if pair.raw != pair.heard { take["raw"] = pair.raw }
            return take
        }
        let object: [String: Any] = ["round": round.rawValue, "languages": policy.allowed, "takes": takes]
        let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys, .withoutEscapingSlashes])) ?? Data("{}".utf8)
        return String(decoding: data, as: UTF8.self)
    }

    private static let russianOnly = Set("ыэъёЫЭЪЁ")

    private static func hasRussianLetters(_ text: String) -> Bool {
        text.contains { russianOnly.contains($0) }
    }

    private static func hasCyrillic(_ text: String) -> Bool {
        text.unicodeScalars.contains { (0x0400...0x04FF).contains($0.value) }
    }

    /// Up to three valid sentences, nil when the reply is malformed or nothing survives the filters.
    public static func parseTargets(_ reply: String, policy: LanguagePolicy) -> [CalibrationPrompt]? {
        guard let object = JSONExtraction.firstObject(in: reply), let items = object["sentences"] as? [Any] else { return nil }
        var strings: [String] = []
        for item in items {
            guard let s = item as? String else { return nil }
            strings.append(s)
        }
        var out: [CalibrationPrompt] = []
        for raw in strings {
            let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty, text.count <= 200 else { continue }
            guard !hasRussianLetters(text), !ScriptCleanup.hasPolishLetters(text) else { continue }
            let cyrillic = hasCyrillic(text)
            if cyrillic && !policy.allowed.contains("uk") { continue }
            let language = cyrillic || !policy.allowed.contains("en") ? "uk" : "en"
            out.append(CalibrationPrompt(id: "target-\(out.count + 1)", text: text, language: language))
            if out.count == 3 { break }
        }
        return out.isEmpty ? nil : out
    }

    /// Notes are required; terms are filtered the way a dictionary entry must look.
    public static func parseProfile(_ reply: String, existingDictionary: [String]) -> CalibrationProfile? {
        guard let object = JSONExtraction.firstObject(in: reply), let rawNotes = object["notes"] as? String else { return nil }
        let notes = rawNotes.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !notes.isEmpty else { return nil }
        var known = Set(existingDictionary.map { $0.lowercased() })
        var terms: [String] = []
        for item in (object["terms"] as? [Any] ?? []) {
            guard let raw = item as? String else { continue }
            let term = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard (2...40).contains(term.count), term.contains(where: \.isLetter) else { continue }
            // A newline inside a term would reach the polishing prompt as part of the dictionary.
            guard !term.unicodeScalars.contains(where: { $0.properties.generalCategory == .control }) else { continue }
            guard term.split(separator: " ").count <= 3 else { continue }
            guard !hasRussianLetters(term), !ScriptCleanup.hasPolishLetters(term) else { continue }
            guard known.insert(term.lowercased()).inserted else { continue }
            terms.append(term)
            if terms.count == 12 { break }
        }
        return CalibrationProfile(notes: notes, terms: terms)
    }

    /// Appends checked terms in order, skipping case-insensitive duplicates, up to the dictionary limit.
    public static func mergeDictionary(existing: [String], adding: [String]) -> [String] {
        var out = existing
        var known = Set(existing.map { $0.lowercased() })
        for term in adding {
            guard out.count < DictionaryLearning.maxTerms else { break }
            if known.insert(term.lowercased()).inserted { out.append(term) }
        }
        return out
    }
}
