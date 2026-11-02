import XCTest
@testable import ShepitCore

final class CalibrationScriptTests: XCTestCase {
    private func count(_ prompts: [CalibrationPrompt], _ language: String) -> Int {
        prompts.filter { $0.language == language }.count
    }

    func testListsFollowPolicy() {
        let both = LanguagePolicy.ukrainianEnglish
        XCTAssertEqual(CalibrationScript.words(for: both).count, 8)
        XCTAssertEqual(count(CalibrationScript.words(for: both), "uk"), 6)
        XCTAssertEqual(count(CalibrationScript.words(for: both), "en"), 2)
        XCTAssertEqual(CalibrationScript.phrases(for: both).count, 5)
        XCTAssertEqual(count(CalibrationScript.phrases(for: both), "uk"), 4)
        XCTAssertEqual(count(CalibrationScript.phrases(for: both), "en"), 1)

        let uk = LanguagePolicy.ukrainian
        XCTAssertEqual(CalibrationScript.words(for: uk).map(\.language), Array(repeating: "uk", count: 8))
        XCTAssertEqual(CalibrationScript.phrases(for: uk).map(\.language), Array(repeating: "uk", count: 5))

        let en = LanguagePolicy.english
        XCTAssertEqual(CalibrationScript.words(for: en).map(\.language), Array(repeating: "en", count: 8))
        XCTAssertEqual(CalibrationScript.phrases(for: en).map(\.language), Array(repeating: "en", count: 5))
    }

    func testFallbackFollowsPolicy() {
        let both = CalibrationScript.fallback(for: .ukrainianEnglish)
        XCTAssertEqual(both.count, 3)
        XCTAssertEqual(count(both, "uk"), 2)
        XCTAssertEqual(count(both, "en"), 1)
        XCTAssertEqual(CalibrationScript.fallback(for: .ukrainian).map(\.language), ["uk", "uk", "uk"])
        XCTAssertEqual(CalibrationScript.fallback(for: .english).map(\.language), ["en", "en", "en"])
    }

    func testNoRussianOrPolishLetters() {
        for policy in LanguagePolicy.allCases {
            let all = CalibrationScript.words(for: policy) + CalibrationScript.phrases(for: policy)
                + CalibrationScript.fallback(for: policy)
            for prompt in all {
                XCTAssertFalse(prompt.text.contains { "ыэъёЫЭЪЁ".contains($0) }, prompt.text)
                XCTAssertFalse(ScriptCleanup.hasPolishLetters(prompt.text), prompt.text)
                XCTAssertFalse(prompt.text.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    func testIdsUnique() {
        for policy in LanguagePolicy.allCases {
            let all = CalibrationScript.words(for: policy) + CalibrationScript.phrases(for: policy)
                + CalibrationScript.fallback(for: policy)
            XCTAssertEqual(Set(all.map(\.id)).count, all.count, "duplicate id under \(policy)")
        }
    }
}
