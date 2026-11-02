import XCTest
@testable import ShepitCore

final class PolishLeakTests: XCTestCase {
    private func first(_ text: String, confidence: Float? = 0.5) -> Transcript {
        fakeTranscript(text, confidence: confidence)
    }

    /// A decode closure backed by a fake engine whose answer depends on the language code.
    private func run(_ firstPass: Transcript, policy: LanguagePolicy, engine: FakeEngine) async -> Transcript {
        await ScriptCleanup.resolvePolishLeak(first: firstPass, policy: policy) { code in
            try await engine.transcribe([0], hints: TranscriptionHints(language: code))
        }
    }

    private func engine(uk: Transcript?, en: Transcript?) -> FakeEngine {
        struct Boom: Error {}
        return FakeEngine(id: .parakeetV3) { _, hints in
            guard let t = hints.language == "uk" ? uk : en else { throw Boom() }
            return t
        }
    }

    func testNoPolishLettersNoRedecode() async {
        let e = engine(uk: fakeTranscript("uk", confidence: 1), en: fakeTranscript("en", confidence: 1))
        let out = await run(first("Привіт світе"), policy: .ukrainianEnglish, engine: e)
        XCTAssertEqual(out.text, "Привіт світе")
        XCTAssertEqual(e.callCount, 0)
    }

    func testPolishLettersRedecodeUkAndEn() async {
        let e = engine(uk: fakeTranscript("зроби", confidence: 0.9), en: fakeTranscript("do", confidence: 0.4))
        let out = await run(first("Zrobię"), policy: .ukrainianEnglish, engine: e)
        XCTAssertEqual(e.callCount, 2)
        XCTAssertEqual(out.text, "зроби")
    }

    func testEnglishWinsWhenMoreConfident() async {
        let e = engine(uk: fakeTranscript("зроби", confidence: 0.3), en: fakeTranscript("do it", confidence: 0.8))
        let out = await run(first("Zrobię"), policy: .ukrainianEnglish, engine: e)
        XCTAssertEqual(out.text, "do it")
    }

    func testTieGoesToUkrainian() async {
        let e = engine(uk: fakeTranscript("зроби", confidence: 0.6), en: fakeTranscript("do it", confidence: 0.6))
        let out = await run(first("Zrobię"), policy: .ukrainianEnglish, engine: e)
        XCTAssertEqual(out.text, "зроби")
    }

    func testSingleLanguagePolicyNeverRedecodes() async {
        for policy in [LanguagePolicy.ukrainian, .english] {
            let e = engine(uk: fakeTranscript("x", confidence: 1), en: fakeTranscript("y", confidence: 1))
            let out = await run(first("Zrobię"), policy: policy, engine: e)
            XCTAssertEqual(out.text, "Zrobię")
            XCTAssertEqual(e.callCount, 0)
        }
    }

    func testFailedRedecodesKeepFirst() async {
        let e = engine(uk: nil, en: nil)
        let out = await run(first("Zrobię"), policy: .ukrainianEnglish, engine: e)
        XCTAssertEqual(out.text, "Zrobię")
    }

    func testFirstPassKeptWhenMostConfident() async {
        let e = engine(uk: fakeTranscript("зроби", confidence: 0.3), en: fakeTranscript("do it", confidence: 0.4))
        let out = await run(first("I met Łukasz", confidence: 0.9), policy: .ukrainianEnglish, engine: e)
        XCTAssertEqual(out.text, "I met Łukasz")
    }
}
