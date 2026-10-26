import XCTest
@testable import ShepitCore

final class SpeechPresenceTests: XCTestCase {
    func testSilenceHasNoSpeech() {
        let silence = [Float](repeating: 0, count: 16_000)
        XCTAssertEqual(SpeechPresence.speechSeconds(in: silence), 0)
    }

    func testBurstInNoiseIsMeasured() {
        var samples = (0..<32_000).map { _ in Float.random(in: -0.002...0.002) }
        for i in 8_000..<16_000 { samples[i] += 0.3 * sinf(Float(i) * 0.1) }
        let seconds = SpeechPresence.speechSeconds(in: samples)
        XCTAssertEqual(seconds, 0.5, accuracy: 0.05)
    }

    func testHallucinationFilter() {
        XCTAssertTrue(HallucinationFilter.isJunk("Дякую за перегляд!"))
        XCTAssertTrue(HallucinationFilter.isJunk("Субтитри створив DimaTorzok"))
        XCTAssertTrue(HallucinationFilter.isJunk("Thank you."))
        XCTAssertFalse(HallucinationFilter.isJunk("Дякую за звіт, подивлюся завтра."))
        XCTAssertFalse(HallucinationFilter.isJunk("Зроби pull request після commit."))
    }
}

final class WhisperLanguageTests: XCTestCase {
    func testSupportedLanguagesAreKept() {
        XCTAssertNil(WhisperKitEngine.retryLanguage(detected: "uk", text: "Тестую введення"))
        XCTAssertNil(WhisperKitEngine.retryLanguage(detected: "en", text: "Testing input"))
        XCTAssertNil(WhisperKitEngine.retryLanguage(detected: nil, text: ""))
    }

    func testOtherLanguagesRetryByScript() {
        XCTAssertEqual(WhisperKitEngine.retryLanguage(detected: "ru", text: "Достаю введення в файла"), "uk")
        XCTAssertEqual(WhisperKitEngine.retryLanguage(detected: "be", text: "Зрабі pull request"), "uk")
        XCTAssertEqual(WhisperKitEngine.retryLanguage(detected: "de", text: "Make a pull request"), "en")
    }
}

final class LivePreviewTests: XCTestCase {
    func testFreezeCutLandsInTheQuietGap() {
        var samples = (0..<(12 * 16_000)).map { i in 0.3 * sinf(Float(i) * 0.05) }
        for i in (9 * 16_000)..<(9 * 16_000 + 3_200) { samples[i] = 0 }
        let cut = LivePreview.quietestCut(in: samples, from: 8 * 16_000, to: 11 * 16_000)
        XCTAssertTrue((9 * 16_000)...(9 * 16_000 + 3_200) ~= cut, "cut at \(cut)")
    }

    func testJoinSkipsEmptyParts() {
        XCTAssertEqual(LivePreview.join("", "a b"), "a b")
        XCTAssertEqual(LivePreview.join("a", ""), "a")
        XCTAssertEqual(LivePreview.join("a", "b"), "a b")
    }
}
