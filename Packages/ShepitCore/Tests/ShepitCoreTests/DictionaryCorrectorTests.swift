import XCTest
@testable import ShepitCore

final class DictionaryCorrectorTests: XCTestCase {
    private let dictionary = ["Claude Code", "handleSubmit", "UserService.swift", "Vercel", "Supabase", "commit",
                              "pull request", "Shepit"]

    func testCyrillicTransliterationsBecomeTerms() {
        XCTAssertEqual(DictionaryCorrector.correct("Відкрий клод код і перевір", dictionary: dictionary),
                       "Відкрий Claude Code і перевір")
        XCTAssertEqual(DictionaryCorrector.correct("Зроби рефакторинг хендл сабміт.", dictionary: dictionary),
                       "Зроби рефакторинг handleSubmit.")
        XCTAssertEqual(DictionaryCorrector.correct("Задеплой на версел", dictionary: dictionary),
                       "Задеплой на Vercel")
        XCTAssertEqual(DictionaryCorrector.correct("Відкрий пул реквест і зроби коміт", dictionary: dictionary),
                       "Відкрий pull request і зроби commit")
        // Inflected forms are left for Claude.
        XCTAssertEqual(DictionaryCorrector.correct("після коміту", dictionary: dictionary), "після коміту")
    }

    func testLatinNearMissOfMultiWordTerm() {
        XCTAssertEqual(DictionaryCorrector.correct("Open Cloud Code please", dictionary: dictionary),
                       "Open Claude Code please")
    }

    func testOrdinaryWordsStay() {
        let plain = "Тестую введення в файл. Дивимося, як воно буде працювати."
        XCTAssertEqual(DictionaryCorrector.correct(plain, dictionary: dictionary), plain)
        XCTAssertEqual(DictionaryCorrector.correct("Покажи код ще раз", dictionary: dictionary), "Покажи код ще раз")
        XCTAssertEqual(DictionaryCorrector.correct("колода карт", dictionary: dictionary), "колода карт")
        // A single English word is taken as meant, even when it sounds like a term.
        XCTAssertEqual(DictionaryCorrector.correct("Use Whisper turbo", dictionary: dictionary), "Use Whisper turbo")
    }

    func testCorrectTermsAreUntouched() {
        let text = "Run handleSubmit in UserService.swift"
        XCTAssertEqual(DictionaryCorrector.correct(text, dictionary: dictionary), text)
    }

    func testSoundKeysAgreeAcrossScripts() {
        XCTAssertEqual(DictionaryCorrector.soundKey("клод"), DictionaryCorrector.soundKey("Claude"))
        XCTAssertEqual(DictionaryCorrector.soundKey("реквест"), DictionaryCorrector.soundKey("request"))
        XCTAssertEqual(DictionaryCorrector.soundKey("супабейс"), DictionaryCorrector.soundKey("Supabase"))
        XCTAssertEqual(DictionaryCorrector.spokenParts("UserService.swift"), ["User", "Service", "swift"])
    }

    func testLearningKeepsProperNounLikeTerms() {
        let reported = ["main", "git rebase", "handleSubmit", "Vercel", "UserService.swift", "vercel", "Привіт",
                        "a very long phrase with many words"]
        XCTAssertEqual(DictionaryLearning.newEntries(from: reported, existing: ["Vercel"]),
                       ["handleSubmit", "UserService.swift"])
    }
}
