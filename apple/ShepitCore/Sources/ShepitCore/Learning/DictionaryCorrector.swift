import Foundation

/// Restores dictionary terms the recognizer misspelled, without a language model. Runs on dictations that
/// skip Claude (fast path, Claude unavailable or failed). Words are compared by a coarse sound key after
/// Cyrillic is transliterated, so "клод код" and "Cloud Code" both match "Claude Code" and "хендл сабміт"
/// matches "handleSubmit".
///
/// Short keys are where false hits come from ("код" is an ordinary Ukrainian word, "колода карт" is close
/// to "Claude Code"), so a term needs a key of at least 5 sounds, keys under 7 must match exactly and longer
/// ones may differ by one sound. Inflected forms ("після коміту") are left for Claude. Latin text is
/// corrected only toward terms of two or more spoken words: a single English word such as "Whisper" is too
/// likely to be meant as written. Words that already spell a term are never part of a correction.
public enum DictionaryCorrector {
    public static func correct(_ text: String, dictionary: [String]) -> String {
        let tokens = tokenize(text)
        guard !tokens.isEmpty, !dictionary.isEmpty else { return text }
        let terms = dictionary.compactMap(Term.init)
        guard !terms.isEmpty else { return text }

        let keys = tokens.map { soundKey($0.text) }
        var windows: [(first: Int, last: Int, term: Term)] = []
        for term in terms {
            for size in 1...min(tokens.count, term.parts) {
                for start in 0...(tokens.count - size) {
                    windows.append((start, start + size - 1, term))
                }
            }
        }

        // Tokens that already spell a term ("handleSubmit", "UserService" + "swift") are kept as written.
        var protected = IndexSet()
        for w in windows where letters(tokens[w.first...w.last].map(\.text).joined()) == w.term.letters {
            protected.insert(integersIn: w.first...w.last)
        }

        var candidates: [Match] = []
        for w in windows where !protected.intersects(integersIn: w.first...w.last) {
            let window = tokens[w.first...w.last]
            if w.term.parts < 2, window.allSatisfy(\.isLatin) { continue }
            guard let score = similarity(keys[w.first...w.last].joined(), w.term.key) else { continue }
            candidates.append(Match(first: w.first, last: w.last, term: w.term.text, score: score))
        }
        guard !candidates.isEmpty else { return text }

        // Best score first, then the longer window, so "клод код" wins over "клод" alone.
        candidates.sort { ($0.score, $0.last - $0.first) > ($1.score, $1.last - $1.first) }
        var taken = IndexSet()
        var chosen: [Match] = []
        for m in candidates where !taken.intersects(integersIn: m.first...m.last) {
            taken.insert(integersIn: m.first...m.last)
            chosen.append(m)
        }

        var result = text
        for m in chosen.sorted(by: { $0.first > $1.first }) {
            let range = tokens[m.first].range.lowerBound..<tokens[m.last].range.upperBound
            result.replaceSubrange(range, with: m.term)
        }
        return result
    }

    // MARK: Matching

    private struct Term {
        let text: String
        let key: String
        let letters: String
        /// Spoken words: "handleSubmit" and "UserService.swift" count their camel-case and dotted parts.
        let parts: Int

        init?(_ raw: String) {
            let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            let spoken = DictionaryCorrector.spokenParts(text)
            let key = spoken.map(DictionaryCorrector.soundKey).joined()
            guard !spoken.isEmpty, key.count >= 5 else { return nil }
            self.text = text
            self.key = key
            self.letters = DictionaryCorrector.letters(text)
            self.parts = spoken.count
        }
    }

    private struct Match {
        let first: Int
        let last: Int
        let term: String
        let score: Double
    }

    private struct Token {
        let text: String
        let range: Range<String.Index>
        var isLatin: Bool { text.unicodeScalars.allSatisfy { $0.isASCII } }
    }

    /// nil when the keys are too far apart to be the same term.
    static func similarity(_ a: String, _ b: String) -> Double? {
        guard a.count >= 5, b.count >= 5 else { return nil }
        if a == b { return 1 }
        guard min(a.count, b.count) >= 7, levenshtein(Array(a), Array(b)) <= 1 else { return nil }
        return 1 - 1 / Double(max(a.count, b.count))
    }

    private static func letters(_ s: String) -> String {
        String(s.lowercased().filter { $0.isLetter || $0.isNumber })
    }

    private static func levenshtein(_ a: [Character], _ b: [Character]) -> Int {
        var previous = Array(0...b.count)
        var current = previous
        for i in 1...a.count {
            current[0] = i
            for j in 1...b.count {
                let cost = a[i - 1] == b[j - 1] ? 0 : 1
                current[j] = min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + cost)
            }
            swap(&previous, &current)
        }
        return previous[b.count]
    }

    private static func tokenize(_ text: String) -> [Token] {
        var tokens: [Token] = []
        var start: String.Index?
        var i = text.startIndex
        while i < text.endIndex {
            let c = text[i]
            let next = text.index(after: i)
            // An apostrophe inside a word belongs to it ("м'ята", "don't").
            let inner = (c == "'" || c == "’" || c == "ʼ") && start != nil && next < text.endIndex && text[next].isLetter
            if c.isLetter || c.isNumber || inner {
                if start == nil { start = i }
            } else if let s = start {
                tokens.append(Token(text: String(text[s..<i]), range: s..<i))
                start = nil
            }
            i = next
        }
        if let s = start { tokens.append(Token(text: String(text[s...]), range: s..<text.endIndex)) }
        return tokens
    }

    static func spokenParts(_ term: String) -> [String] {
        var parts: [String] = []
        var current = ""
        var previous: Character?
        for c in term {
            if !(c.isLetter || c.isNumber) {
                if !current.isEmpty { parts.append(current) }
                current = ""
            } else {
                if let p = previous, p.isLowercase, c.isUppercase, !current.isEmpty {
                    parts.append(current)
                    current = ""
                }
                current.append(c)
            }
            previous = c
        }
        if !current.isEmpty { parts.append(current) }
        return parts
    }

    // MARK: Sound key

    /// A coarse pronunciation key: consonants in loose classes, every vowel run as "A", repeats collapsed.
    /// English spelling quirks that matter for terms spoken by Ukrainians are handled first (silent final
    /// "e", "qu" said as "кв", soft "c").
    static func soundKey(_ word: String) -> String {
        var s = transliterate(word.lowercased())
        if word.unicodeScalars.allSatisfy({ $0.isASCII }) { s = dropSilentE(s) }
        let digraphs: [(String, String)] = [
            ("shch", "J"), ("sch", "J"), ("tch", "J"), ("dzh", "J"), ("sh", "J"), ("ch", "J"), ("zh", "J"),
            ("ph", "F"), ("th", "T"), ("kh", "K"), ("ck", "K"), ("qu", "KF"), ("ts", "S"), ("x", "KS"),
            ("ce", "Se"), ("ci", "Si"), ("cy", "Sy"),
        ]
        for (from, to) in digraphs { s = s.replacingOccurrences(of: from, with: to) }
        var key = ""
        for c in s {
            let mapped: Character?
            switch c {
            case "a", "e", "i", "o", "u", "y": mapped = "A"
            case "b", "p": mapped = "P"
            case "f", "v", "w": mapped = "F"
            case "g", "h", "k", "c", "q", "K": mapped = "K"
            case "d", "t", "T": mapped = "T"
            case "s", "z", "S": mapped = "S"
            case "j", "J": mapped = "J"
            case "F": mapped = "F"
            case "l", "m", "n", "r": mapped = Character(c.uppercased())
            default: mapped = c.isNumber ? c : nil
            }
            guard let mapped, key.last != mapped else { continue }
            key.append(mapped)
        }
        return key
    }

    private static func dropSilentE(_ s: String) -> String {
        // "code", "claude", "supabase", "handle": the final "e" is not spoken.
        guard s.count > 3, s.hasSuffix("e") else { return s }
        let beforeLast = s[s.index(s.endIndex, offsetBy: -2)]
        return "aeiouy".contains(beforeLast) ? s : String(s.dropLast())
    }

    private static let cyrillic: [Character: String] = [
        "а": "a", "б": "b", "в": "v", "г": "h", "ґ": "g", "д": "d", "е": "e", "є": "ye", "ж": "zh", "з": "z",
        "и": "y", "і": "i", "ї": "yi", "й": "y", "к": "k", "л": "l", "м": "m", "н": "n", "о": "o", "п": "p",
        "р": "r", "с": "s", "т": "t", "у": "u", "ф": "f", "х": "kh", "ц": "ts", "ч": "ch", "ш": "sh",
        "щ": "shch", "ь": "", "ю": "yu", "я": "ya", "ы": "y", "э": "e", "ё": "yo", "ъ": "",
        "'": "", "’": "", "ʼ": "",
    ]

    private static func transliterate(_ s: String) -> String {
        var out = ""
        for c in s {
            if let mapped = cyrillic[c] { out += mapped } else { out.append(c) }
        }
        return out
    }
}

/// Picks dictionary candidates out of the terms Claude reports as fixed. Claude's list is noisy (it has
/// listed "main" and "git rebase"), so only proper-noun-like tokens are kept: a capital letter, a dot, an
/// underscore or a digit inside, Latin script, at most three words.
public enum DictionaryLearning {
    public static let maxTerms = 300

    public static func newEntries(from reported: [String], existing: [String]) -> [String] {
        var known = Set(existing.map { $0.lowercased() })
        var out: [String] = []
        for raw in reported {
            let term = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard (2...40).contains(term.count), !known.contains(term.lowercased()) else { continue }
            guard term.unicodeScalars.allSatisfy({ $0.isASCII }), term.contains(where: \.isLetter) else { continue }
            // A newline or NUL inside a term would go back to the agent as part of the dictionary prompt.
            guard !term.unicodeScalars.contains(where: { $0.properties.generalCategory == .control }) else { continue }
            guard term.split(separator: " ").count <= 3 else { continue }
            let marked = term.contains(where: { $0.isUppercase || $0.isNumber || $0 == "." || $0 == "_" })
            guard marked else { continue }
            known.insert(term.lowercased())
            out.append(term)
        }
        return Array(out.prefix(max(0, maxTerms - existing.count)))
    }
}
