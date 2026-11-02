import Foundation

/// One thing the speaker reads aloud. `language` is "uk" or "en".
public struct CalibrationPrompt: Sendable, Equatable {
    public var id: String
    public var text: String
    public var language: String

    public init(id: String, text: String, language: String) {
        self.id = id
        self.text = text
        self.language = language
    }
}

/// The fixed reading lists. Words and phrases cover sounds recognizers get wrong for this speaker (weak р,
/// swallowed л, soft signs, English terms in Ukrainian sentences). No Russian or Polish letters anywhere.
public enum CalibrationScript {
    private static func make(_ prefix: String, _ language: String, _ texts: [String]) -> [CalibrationPrompt] {
        texts.enumerated().map { CalibrationPrompt(id: "\(prefix)-\(language)-\($0.offset + 1)", text: $0.element, language: language) }
    }

    private static let ukWords = make("word", "uk", ["рефакторинг", "репозиторій", "пул реквест", "деплой",
                                                      "лінтер", "щоправда", "п'ятниця", "черговий"])
    private static let enWords = make("word", "en", ["branch", "commit", "rollback", "throughput",
                                                      "regular", "world", "lightweight", "repository"])
    private static let ukTech = make("tech", "uk", ["Зроби рефакторинг цього модуля і відкрий пул реквест.",
                                                     "Після деплою лінтер знайшов три помилки в репозиторії."])
    private static let ukPlain = make("plain", "uk", ["Рано вранці Роман приїхав на роботу трамваєм.",
                                                       "Хлопці довго чекали на лист від Олега.",
                                                       "Щоправда, у п'ятницю ми залишимося вдома."])
    private static let enPhrases = make("phrase", "en", ["Please review the pull request before lunch.",
                                                          "The build failed because of a missing dependency.",
                                                          "Roll back the last release and rerun the tests.",
                                                          "Laura really likes the new library.",
                                                          "We merged three branches this morning."])
    private static let ukFallback = make("fallback", "uk", ["Рома розробив реліз і швидко закомітив виправлення.",
                                                             "Лілія повільно читала лист біля вікна.",
                                                             "Чотири щасливі черепахи шукали щавель у полі."])
    private static let enFallback = make("fallback", "en", ["Rerun the regression tests on the release branch.",
                                                             "Lily left the little laptop on the shelf.",
                                                             "The throughput dropped after the third deploy."])

    /// Both languages: six Ukrainian words and the two English ones that test th and w. One language: its eight.
    public static func words(for policy: LanguagePolicy) -> [CalibrationPrompt] {
        switch policy {
        case .ukrainianEnglish: return Array(ukWords.prefix(6)) + [enWords[3], enWords[5]]
        case .ukrainian: return ukWords
        case .english: return enWords
        }
    }

    /// Both languages: two technical and two plain Ukrainian phrases, then one English. Always five.
    public static func phrases(for policy: LanguagePolicy) -> [CalibrationPrompt] {
        switch policy {
        case .ukrainianEnglish: return ukTech + Array(ukPlain.prefix(2)) + [enPhrases[0]]
        case .ukrainian: return ukTech + ukPlain
        case .english: return enPhrases
        }
    }

    /// Used when no agent can pick sentences for the third stage.
    public static func fallback(for policy: LanguagePolicy) -> [CalibrationPrompt] {
        switch policy {
        case .ukrainianEnglish: return Array(ukFallback.prefix(2)) + [enFallback[0]]
        case .ukrainian: return ukFallback
        case .english: return enFallback
        }
    }
}
