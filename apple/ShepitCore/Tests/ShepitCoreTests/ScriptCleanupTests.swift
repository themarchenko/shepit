import XCTest
@testable import ShepitCore

final class ScriptCleanupTests: XCTestCase {
    func testUkrainianLetters() {
        XCTAssertEqual(ScriptCleanup.ukrainianLetters("ы э ъ ё"), "и е \u{02BC} е")
        XCTAssertEqual(ScriptCleanup.ukrainianLetters("Ы Э Ъ Ё"), "И Е \u{02BC} Е")
        XCTAssertEqual(ScriptCleanup.ukrainianLetters("Привіт, світе і hello 123"), "Привіт, світе і hello 123")
    }

    func testFinalTranscriptMapsTextOnly() {
        let t = Transcript(text: "мы э", language: "uk", audioSeconds: 3, processingSeconds: 1.5, confidence: 0.8)
        let out = ScriptCleanup.finalTranscript(t)
        XCTAssertEqual(out.text, "ми е")
        XCTAssertEqual(out.language, "uk")
        XCTAssertEqual(out.audioSeconds, 3)
        XCTAssertEqual(out.processingSeconds, 1.5)
        XCTAssertEqual(out.confidence, 0.8)
    }

    func testPolishLetters() {
        for letter in "ąćęłńśźżĄĆĘŁŃŚŹŻ" {
            XCTAssertTrue(ScriptCleanup.hasPolishLetters("słowo \(letter)"), "\(letter)")
        }
        XCTAssertFalse(ScriptCleanup.hasPolishLetters("ó"))
        XCTAssertFalse(ScriptCleanup.hasPolishLetters("Привіт, світе їжак"))
        XCTAssertFalse(ScriptCleanup.hasPolishLetters("plain English text"))
    }
}
