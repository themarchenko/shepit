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
