import XCTest
@testable import ShepitCore

@MainActor
final class ProgressiveHeadTests: XCTestCase {
    private let second = 16_000

    private func whisper(_ responder: @escaping FakeEngine.Responder) -> FakeEngine {
        FakeEngine(id: .whisperTurbo, responder: responder)
    }

    func testHeadPublishedWithTextAndEndSample() async {
        let engine = whisper { _, _ in fakeTranscript("w1", language: "uk") }
        let source = FakeLiveSource(count: 15 * second, generator: FakeLiveSource.speech)
        let pieces = ProgressiveTranscriber()
        XCTAssertNil(pieces.head)
        pieces.start(source: source, engine: { engine }, hints: TranscriptionHints())
        defer { pieces.cancel() }
        let got = await waitUntil { pieces.head != nil }
        XCTAssertTrue(got)
        guard let head = pieces.head else { return }
        XCTAssertEqual(head.text, "w1")
        XCTAssertEqual(head.language, "uk")
        XCTAssertTrue((7 * second)...(12 * second) ~= head.samples, "cut at \(head.samples)")
    }

    func testSilentPieceAdvancesHeadWithoutText() async {
        let engine = whisper { _, _ in fakeTranscript("w1", language: "uk") }
        let source = FakeLiveSource(count: 15 * second, generator: FakeLiveSource.speech(forSeconds: 8))
        let pieces = ProgressiveTranscriber()
        pieces.start(source: source, engine: { engine }, hints: TranscriptionHints())
        defer { pieces.cancel() }
        let first = await waitUntil { pieces.head != nil }
        XCTAssertTrue(first)
        guard let before = pieces.head else { return }
        XCTAssertEqual(before.text, "w1")

        source.count = 30 * second
        let advanced = await waitUntil { (pieces.head?.samples ?? 0) > before.samples }
        XCTAssertTrue(advanced, "head stayed at \(before.samples)")
        XCTAssertEqual(pieces.head?.text, "w1", "a silent piece must not change the text")
        XCTAssertEqual(engine.callCount, 1, "silence must not reach the engine")
    }

    func testFailedPieceKeepsLastHead() async {
        struct Boom: Error {}
        // First call succeeds, later ones throw.
        let calls = OrderedCallCounter()
        let flaky = whisper { _, _ in
            if calls.next() == 1 { return fakeTranscript("w1", language: "uk") }
            throw Boom()
        }
        let source = FakeLiveSource(count: 15 * second, generator: FakeLiveSource.speech)
        let pieces = ProgressiveTranscriber()
        pieces.start(source: source, engine: { flaky }, hints: TranscriptionHints())
        defer { pieces.cancel() }
        let first = await waitUntil { pieces.head != nil }
        XCTAssertTrue(first)
        let kept = pieces.head

        source.count = 30 * second
        let second = await waitUntil { flaky.callCount >= 2 }
        XCTAssertTrue(second)
        try? await Task.sleep(for: .seconds(0.3))
        XCTAssertEqual(pieces.head, kept)
        XCTAssertEqual(pieces.head?.text, "w1")
    }

    func testFinalTextIsNotMappedOrCorrected() async {
        let engine = whisper { _, _ in fakeTranscript("мы клод код", language: "uk") }
        let generator = FakeLiveSource.speech(forSeconds: 8)
        let source = FakeLiveSource(count: 15 * second, generator: generator)
        let pieces = ProgressiveTranscriber()
        pieces.start(source: source, engine: { engine },
                     hints: TranscriptionHints(vocabulary: ["Claude Code"]))
        let got = await waitUntil { pieces.head != nil }
        XCTAssertTrue(got)
        let samples = source.capturedSamples(from: 0)
        let result = await pieces.complete(samples)
        XCTAssertEqual(result?.transcript.text, "мы клод код")
    }

    /// Language the second piece was decoded with, given what the first piece reported.
    private func secondPieceLanguage(firstReports: String) async -> String? {
        let seen = HintLog()
        let engine = whisper { _, hints in
            seen.add(hints.language)
            return fakeTranscript("w", language: firstReports)
        }
        let source = FakeLiveSource(count: 15 * second, generator: FakeLiveSource.speech)
        let pieces = ProgressiveTranscriber()
        pieces.start(source: source, engine: { engine }, hints: TranscriptionHints())
        defer { pieces.cancel() }
        _ = await waitUntil { pieces.head != nil }
        source.count = 40 * second
        _ = await waitUntil { engine.callCount >= 2 }
        return seen.second
    }

    func testOutsideLanguageNotAdopted() async {
        for outside in ["ru", "pl"] {
            let lang = await secondPieceLanguage(firstReports: outside)
            XCTAssertNil(lang, "\(outside) must not become the language of later pieces")
        }
    }

    func testAllowedLanguageAdopted() async {
        let lang = await secondPieceLanguage(firstReports: "en")
        XCTAssertEqual(lang, "en")
    }
}

final class HintLog: @unchecked Sendable {
    private let lock = NSLock()
    private var languages: [String?] = []
    func add(_ l: String?) { lock.withLock { languages.append(l) } }
    var second: String? { lock.withLock { languages.count >= 2 ? languages[1] : nil } }
}

final class OrderedCallCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var n = 0
    func next() -> Int {
        lock.withLock {
            n += 1
            return n
        }
    }
}
