import XCTest
@testable import ShepitCore

final class LanguagePolicyTests: XCTestCase {
    func testStoredValuesMigrate() {
        for stored in [nil, "auto", "", "fr", "UK"] as [String?] {
            XCTAssertEqual(LanguagePolicy(stored: stored), .ukrainianEnglish, "\(stored ?? "nil")")
        }
        XCTAssertEqual(LanguagePolicy(stored: "uk"), .ukrainian)
        XCTAssertEqual(LanguagePolicy(stored: "en"), .english)
        XCTAssertEqual(LanguagePolicy(stored: "uk+en"), .ukrainianEnglish)
    }

    func testAllowedAndHint() {
        XCTAssertEqual(LanguagePolicy.ukrainianEnglish.allowed, ["uk", "en"])
        XCTAssertEqual(LanguagePolicy.ukrainian.allowed, ["uk"])
        XCTAssertEqual(LanguagePolicy.english.allowed, ["en"])
        XCTAssertNil(LanguagePolicy.ukrainianEnglish.hintLanguage)
        XCTAssertEqual(LanguagePolicy.ukrainian.hintLanguage, "uk")
        XCTAssertEqual(LanguagePolicy.english.hintLanguage, "en")
    }

    func testRussianPolishNeverAllowed() {
        for policy in LanguagePolicy.allCases {
            XCTAssertFalse(policy.isAllowed("ru"))
            XCTAssertFalse(policy.isAllowed("pl"))
            XCTAssertFalse(policy.isAllowed(nil))
        }
        XCTAssertTrue(LanguagePolicy.ukrainianEnglish.isAllowed("uk"))
        XCTAssertTrue(LanguagePolicy.ukrainianEnglish.isAllowed("en"))
        XCTAssertFalse(LanguagePolicy.ukrainian.isAllowed("en"))
        XCTAssertFalse(LanguagePolicy.english.isAllowed("uk"))
    }

    func testPickBestOfAllowed() {
        XCTAssertEqual(LanguagePolicy.ukrainianEnglish.pick(fromProbabilities: ["ru": 0.9, "uk": 0.04, "en": 0.06]), "en")
    }

    func testPickTieGoesToUkrainian() {
        XCTAssertEqual(LanguagePolicy.ukrainianEnglish.pick(fromProbabilities: ["uk": 0.4, "en": 0.4]), "uk")
    }

    func testPickNilWhenNoAllowedKey() {
        XCTAssertNil(LanguagePolicy.ukrainianEnglish.pick(fromProbabilities: ["ru": 0.9]))
    }

    func testPickSingleLanguage() {
        XCTAssertEqual(LanguagePolicy.ukrainian.pick(fromProbabilities: ["en": 0.9, "uk": 0.1]), "uk")
        XCTAssertEqual(LanguagePolicy.english.pick(fromProbabilities: ["uk": 0.9]), "en")
    }

    func testFormatterHint() {
        XCTAssertEqual(LanguagePolicy.ukrainianEnglish.formatterHint(reported: "en"), "en")
        XCTAssertNil(LanguagePolicy.ukrainianEnglish.formatterHint(reported: nil))
        XCTAssertEqual(LanguagePolicy.ukrainian.formatterHint(reported: "en"), "uk")
        XCTAssertEqual(LanguagePolicy.english.formatterHint(reported: nil), "en")
    }

    func testHintsPolicy() {
        XCTAssertEqual(TranscriptionHints(language: nil).policy, .ukrainianEnglish)
        XCTAssertEqual(TranscriptionHints(language: "uk").policy, .ukrainian)
        XCTAssertEqual(TranscriptionHints(language: "en").policy, .english)
        XCTAssertEqual(TranscriptionHints(language: "de").policy, .ukrainianEnglish)
    }

    func testDisplayNames() {
        XCTAssertEqual(LanguagePolicy.ukrainianEnglish.displayName, "Ukrainian + English")
        XCTAssertEqual(LanguagePolicy.ukrainian.displayName, "Ukrainian")
        XCTAssertEqual(LanguagePolicy.english.displayName, "English")
    }
}
