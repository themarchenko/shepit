import XCTest
@testable import ShepitCore

@MainActor
final class LivePreviewHeadTests: XCTestCase {
    private let second = 16_000

    private func positionEngine() -> FakeEngine {
        FakeEngine(id: .parakeetV3) { samples, _ in fakeTranscript(positionText(samples)) }
    }

    private func run(_ preview: LivePreview, source: FakeLiveSource, engine: FakeEngine,
                     language: String? = "uk", dictionary: [String] = [],
                     head: @escaping @MainActor () -> PreviewHead? = { nil }) {
        preview.start(source: source, engine: { engine }, hints: TranscriptionHints(language: language),
                      head: head, dictionary: dictionary)
    }

    func testWhisperHeadReplacesParakeetFrozenText() async {
        let source = FakeLiveSource(count: 13 * second)
        let preview = LivePreview()
        var head: PreviewHead?
        run(preview, source: source, engine: positionEngine(), head: { head })
        defer { preview.stop() }
        let froze = await waitUntil { preview.text.stable.contains("p0_") }
        XCTAssertTrue(froze, "Parakeet never froze its own text: \(preview.text)")

        head = PreviewHead(text: "WHISPER", samples: 5 * second, language: "uk")
        let adopted = await waitUntil { preview.text.full.hasPrefix("WHISPER p80000_") }
        XCTAssertTrue(adopted, "head not adopted: \(preview.text)")
        XCTAssertFalse(preview.text.full.contains("p0_"), "Parakeet's frozen text stayed: \(preview.text)")
    }

    func testNoHeadStaysParakeetOnly() async {
        let source = FakeLiveSource(count: 5 * second)
        let preview = LivePreview()
        run(preview, source: source, engine: positionEngine())
        defer { preview.stop() }
        let shown = await waitUntil { preview.text.full.hasPrefix("p0_") }
        XCTAssertTrue(shown, "\(preview.text)")
    }

    func testEmptyHeadSkipsSamplesWithoutText() async {
        let source = FakeLiveSource(count: 13 * second)
        let preview = LivePreview()
        run(preview, source: source, engine: positionEngine(),
            head: { PreviewHead(text: "", samples: 5 * 16_000, language: "uk") })
        defer { preview.stop() }
        let shown = await waitUntil { preview.text.full.hasPrefix("p80000_") }
        XCTAssertTrue(shown, "\(preview.text)")
    }

    func testHeadShownWhenTailIsUnderOneSecond() async {
        let count = 5 * second
        let source = FakeLiveSource(count: count)
        let preview = LivePreview()
        run(preview, source: source, engine: positionEngine(),
            head: { PreviewHead(text: "WHISPER", samples: count - 8_000, language: "uk") })
        defer { preview.stop() }
        let shown = await waitUntil { preview.text.full == "WHISPER" }
        XCTAssertTrue(shown, "\(preview.text)")
    }

    func testUkrainianLetterMap() {
        XCTAssertEqual(LivePreview.ukrainianLetters("ы э ъ ё"), "и е \u{02BC} е")
        XCTAssertEqual(LivePreview.ukrainianLetters("Ы Э Ъ Ё"), "И Е \u{02BC} Е")
        XCTAssertEqual(LivePreview.ukrainianLetters("Привіт, світе і hello 123"), "Привіт, світе і hello 123")
    }

    private func autoEngine(ukText: String, enText: String, ukScore: Float, enScore: Float) -> FakeEngine {
        FakeEngine(id: .parakeetV3) { _, hints in
            hints.language == "uk" ? fakeTranscript(ukText, confidence: ukScore)
                                   : fakeTranscript(enText, confidence: enScore)
        }
    }

    private func shown(language: String?, engine: FakeEngine, seconds: Int = 4) async -> String {
        let preview = LivePreview()
        run(preview, source: FakeLiveSource(count: seconds * second), engine: engine, language: language)
        defer { preview.stop() }
        await waitUntil { !preview.text.isEmpty }
        return preview.text.full
    }

    func testLettersMappedOnlyWhenPassLanguageIsUk() async {
        let same = FakeEngine(id: .parakeetV3) { _, _ in fakeTranscript("мы") }
        let locked = await shown(language: "uk", engine: same, seconds: 2)
        XCTAssertEqual(locked, "ми")
        let lockedEn = await shown(language: "en", engine: same, seconds: 2)
        XCTAssertEqual(lockedEn, "мы")

        let ukWins = await shown(language: nil, engine: autoEngine(ukText: "мы", enText: "мы", ukScore: 0.9, enScore: 0.1))
        XCTAssertEqual(ukWins, "ми")
        let enWins = await shown(language: nil, engine: autoEngine(ukText: "мы", enText: "мы", ukScore: 0.1, enScore: 0.9))
        XCTAssertEqual(enWins, "мы")
    }

    func testHeadLettersFollowHeadLanguage() async {
        for (language, expected) in [("uk", "ми"), ("en", "мы"), (nil, "мы")] as [(String?, String)] {
            let count = 5 * second
            let preview = LivePreview()
            run(preview, source: FakeLiveSource(count: count), engine: positionEngine(),
                head: { PreviewHead(text: "мы", samples: count - 8_000, language: language) })
            let got = await waitUntil { !preview.text.isEmpty }
            XCTAssertTrue(got)
            XCTAssertEqual(preview.text.full, expected, "head language \(language ?? "nil")")
            preview.stop()
        }
    }

    func testDictionaryAppliedToPassAndHead() async {
        let dictionary = ["Claude Code", "handleSubmit"]
        let pass = LivePreview()
        let engine = FakeEngine(id: .parakeetV3) { _, _ in fakeTranscript("клод код працює") }
        run(pass, source: FakeLiveSource(count: 2 * second), engine: engine, dictionary: dictionary)
        let gotPass = await waitUntil { !pass.text.isEmpty }
        pass.stop()
        XCTAssertTrue(gotPass)
        XCTAssertTrue(pass.text.full.contains("Claude Code"), pass.text.full)

        let count = 5 * second
        let withHead = LivePreview()
        run(withHead, source: FakeLiveSource(count: count), engine: positionEngine(), dictionary: dictionary,
            head: { PreviewHead(text: "хендл сабміт", samples: count - 8_000, language: "uk") })
        let gotHead = await waitUntil { !withHead.text.isEmpty }
        withHead.stop()
        XCTAssertTrue(gotHead)
        XCTAssertTrue(withHead.text.full.contains("handleSubmit"), withHead.text.full)
    }

    func testLaterHeadReplacesEarlierOne() async {
        let count = 5 * second
        let preview = LivePreview()
        var head: PreviewHead? = PreviewHead(text: "ONE", samples: count - 8_000, language: "uk")
        run(preview, source: FakeLiveSource(count: count), engine: positionEngine(), head: { head })
        defer { preview.stop() }
        let first = await waitUntil { preview.text.full == "ONE" }
        XCTAssertTrue(first, "\(preview.text)")
        head = PreviewHead(text: "ONE TWO", samples: count - 4_000, language: "uk")
        let second = await waitUntil { preview.text.full == "ONE TWO" }
        XCTAssertTrue(second, "\(preview.text)")
    }

    func testHeadNotNewerThanCurrentIsIgnored() async {
        let count = 5 * second
        let preview = LivePreview()
        var head: PreviewHead? = PreviewHead(text: "NEW", samples: count - 4_000, language: "uk")
        run(preview, source: FakeLiveSource(count: count), engine: positionEngine(), head: { head })
        defer { preview.stop() }
        let first = await waitUntil { preview.text.full == "NEW" }
        XCTAssertTrue(first, "\(preview.text)")
        head = PreviewHead(text: "SAME END", samples: count - 4_000, language: "uk")
        try? await Task.sleep(for: .seconds(1.2))
        XCTAssertEqual(preview.text.full, "NEW")
        head = PreviewHead(text: "OLDER", samples: count - 8_000, language: "uk")
        try? await Task.sleep(for: .seconds(1.2))
        XCTAssertEqual(preview.text.full, "NEW")
    }

    /// An engine whose answer, once `slow` is set, arrives after `delay` and says "late".
    private func slowableEngine(_ gate: SlowGate, delay: TimeInterval = 0.4) -> FakeEngine {
        FakeEngine(id: .parakeetV3) { _, _ in
            if gate.isSlow {
                gate.markEntered()
                Thread.sleep(forTimeInterval: delay)
                return fakeTranscript("late")
            }
            return fakeTranscript("early")
        }
    }

    func testClearWhilePassInFlightPublishesNothing() async {
        let gate = SlowGate()
        let preview = LivePreview()
        run(preview, source: FakeLiveSource(count: 2 * second), engine: slowableEngine(gate))
        let shown = await waitUntil { preview.text.full == "early" }
        XCTAssertTrue(shown, "\(preview.text)")
        gate.makeSlow()
        let entered = await waitUntil { gate.hasEntered }
        XCTAssertTrue(entered)
        preview.clear()
        try? await Task.sleep(for: .seconds(1))
        XCTAssertTrue(preview.text.isEmpty, "a cleared preview got text back: \(preview.text)")
    }

    func testStopWhilePassInFlightKeepsText() async {
        let gate = SlowGate()
        let preview = LivePreview()
        run(preview, source: FakeLiveSource(count: 2 * second), engine: slowableEngine(gate))
        let shown = await waitUntil { preview.text.full == "early" }
        XCTAssertTrue(shown, "\(preview.text)")
        gate.makeSlow()
        let entered = await waitUntil { gate.hasEntered }
        XCTAssertTrue(entered)
        preview.stop()
        try? await Task.sleep(for: .seconds(1))
        XCTAssertEqual(preview.text.full, "early")
    }
}

private final class SlowGate: @unchecked Sendable {
    private let lock = NSLock()
    private var slow = false
    private var entered = false
    var isSlow: Bool { lock.withLock { slow } }
    var hasEntered: Bool { lock.withLock { entered } }
    func makeSlow() { lock.withLock { slow = true } }
    func markEntered() { lock.withLock { entered = true } }
}
