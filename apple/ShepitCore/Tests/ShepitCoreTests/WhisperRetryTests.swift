import XCTest
@testable import ShepitCore

final class WhisperRetryTests: XCTestCase {
    private let both = LanguagePolicy.ukrainianEnglish

    private func retry(_ detected: String?, _ text: String, _ p: [String: Float]?, _ policy: LanguagePolicy? = nil) -> String? {
        WhisperKitEngine.retryLanguage(detected: detected, text: text, probabilities: p, policy: policy ?? both)
    }

    func testAllowedDetectionKeeps() {
        XCTAssertNil(retry("uk", "Привіт", ["uk": 0.1, "en": 0.9]))
        XCTAssertNil(retry("en", "Hello", nil))
    }

    func testPolishDetectionUsesProbabilities() {
        XCTAssertEqual(retry("pl", "Zrobię", ["uk": 0.7, "en": 0.2]), "uk")
    }

    /// Whisper's uk/en logits lean English, and on a user recording that came out Russian they picked English,
    /// which translated the Ukrainian speech. A Cyrillic first pass therefore retries as Ukrainian.
    func testCyrillicFirstPassStaysUkrainian() {
        XCTAssertEqual(retry("ru", "Привет мир", ["uk": 0.3, "en": 0.6]), "uk")
    }

    func testLatinFirstPassUsesProbabilities() {
        XCTAssertEqual(retry("pl", "Zrobię to", ["uk": 0.2, "en": 0.7]), "en")
        XCTAssertEqual(retry("pl", "Zrobię to", ["uk": 0.7, "en": 0.2]), "uk")
    }

    func testProbeOnlyForLatinFirstPass() {
        XCTAssertFalse(WhisperKitEngine.needsProbe(detected: "ru", text: "Привет", policy: .ukrainianEnglish))
        XCTAssertTrue(WhisperKitEngine.needsProbe(detected: "pl", text: "Zrobię", policy: .ukrainianEnglish))
        XCTAssertFalse(WhisperKitEngine.needsProbe(detected: "uk", text: "Zrobię", policy: .ukrainianEnglish))
        XCTAssertFalse(WhisperKitEngine.needsProbe(detected: nil, text: "Zrobię", policy: .ukrainianEnglish))
    }

    func testNoProbabilitiesFallsBackToScript() {
        XCTAssertEqual(retry("ru", "Привет мир", nil), "uk")
        XCTAssertEqual(retry("pl", "Zrobię to", nil), "en")
    }

    func testProbabilitiesWithoutAllowedKeysFallBack() {
        XCTAssertEqual(retry("ru", "Привет", ["ru": 0.9]), "uk")
        XCTAssertEqual(retry("de", "Hallo", [:]), "en")
    }

    func testRetryNeverLeavesAllowedSet() {
        let detected: [String?] = ["ru", "pl", "be", "de", "fr", "cs", "sk", "bg", "sr", nil]
        let probabilities: [[String: Float]?] = [nil, [:], ["ru": 0.99], ["uk": 0.2, "en": 0.1], ["en": 1]]
        let texts = ["Привет мир", "Hello world", "Zrobię to", ""]
        for policy in LanguagePolicy.allCases {
            for d in detected {
                for p in probabilities {
                    for text in texts {
                        let r = retry(d, text, p, policy)
                        if let r {
                            XCTAssertTrue(policy.allowed.contains(r), "\(policy) \(d ?? "nil") \(String(describing: p)) -> \(r)")
                        }
                    }
                }
            }
        }
    }

    func testLogitSoftmax() {
        let even = WhisperKitEngine.languageProbabilities(fromLogits: ["uk": 1.5, "en": 1.5])!
        XCTAssertEqual(even["uk"] ?? 0, 0.5, accuracy: 1e-5)
        XCTAssertEqual(even["en"] ?? 0, 0.5, accuracy: 1e-5)
        let skew = WhisperKitEngine.languageProbabilities(fromLogits: ["uk": 3, "en": 1])!
        XCTAssertEqual((skew["uk"] ?? 0) + (skew["en"] ?? 0), 1, accuracy: 1e-5)
        XCTAssertGreaterThan(skew["uk"] ?? 0, skew["en"] ?? 1)
    }

    func testLogitsNeedBothLanguages() {
        XCTAssertNil(WhisperKitEngine.languageProbabilities(fromLogits: ["uk": 3]))
        XCTAssertNil(WhisperKitEngine.languageProbabilities(fromLogits: ["en": 3]))
        XCTAssertNil(WhisperKitEngine.languageProbabilities(fromLogits: [:]))
    }
}
