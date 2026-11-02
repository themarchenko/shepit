import AppKit
import XCTest
@testable import ShepitCore

private final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var n = 0
    var count: Int { lock.withLock { n } }
    func hit() { lock.withLock { n += 1 } }
}

private final class HintsBox: @unchecked Sendable {
    private let lock = NSLock()
    private var value: [TranscriptionHints] = []
    var all: [TranscriptionHints] { lock.withLock { value } }
    func add(_ h: TranscriptionHints) { lock.withLock { value.append(h) } }
}

private struct CountingFormatter: ShepitCore.Formatter {
    let calls: Counter
    func format(_ request: FormatRequest) async throws -> FormatResult {
        calls.hit()
        return FormatResult(text: "from agent", source: .claude)
    }
    func warmUp() async {}
}

/// Drives `DictationSession.finish` with a capture target set, the way `stopRecording` does after a press that
/// began while `captureSink` was set. The recorder is never started, so no microphone and no recording file.
@MainActor
final class CalibrationCaptureTests: XCTestCase {
    private var pasteboard: NSPasteboard!
    private var storeURL: URL!
    private var history: HistoryStore!
    private var session: DictationSession!
    private var settings: AppSettings!
    private var pastes = 0
    private let formatterCalls = Counter()
    private let hints = HintsBox()
    private var engine: FakeEngine!
    private var takes: [CalibrationTake] = []

    private let speech: [Float] = (0..<32_000).map(FakeLiveSource.speech)

    private func makeSession(reply: @escaping @Sendable (TranscriptionHints) -> String) async throws {
        pasteboard = NSPasteboard(name: NSPasteboard.Name("com.themarchenko.shepit.tests.\(UUID().uuidString)"))
        pasteboard.clearContents()
        storeURL = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-test-\(UUID().uuidString).sqlite")
        history = try HistoryStore(url: storeURL)
        pastes = 0
        takes = []
        let inserter = PasteInserter(pasteboard: pasteboard)
        inserter.isTrusted = { true }
        inserter.restoreDelay = .milliseconds(10)
        inserter.sendPaste = { [unowned self] in pastes += 1 }
        settings = AppSettings(defaults: UserDefaults(suiteName: UUID().uuidString)!)
        settings.dictionary = ["Shepit"]
        let box = hints
        engine = FakeEngine(id: settings.engine) { _, h in
            box.add(h)
            return fakeTranscript(reply(h))
        }
        let fake = engine!
        let formatting = FormattingService(claude: CountingFormatter(calls: formatterCalls))
        await formatting.setFastPath(maxWords: 0)
        session = DictationSession(recorder: AudioRecorder(), engines: EngineManager(factory: { _ in fake }),
                                   formatting: formatting, inserter: inserter, history: history, settings: settings)
        session.captureTarget = { [unowned self] take in takes.append(take) }
    }

    override func tearDown() async throws {
        pasteboard?.releaseGlobally()
        if let storeURL {
            for suffix in ["", "-wal", "-shm", "-journal"] {
                try? FileManager.default.removeItem(atPath: storeURL.path + suffix)
            }
        }
    }

    func testCaptureNeverInsertsFormatsOrSavesHistory() async throws {
        let text = "Ми вирішили ы перевірити цей новий модуль у п'ятницю вранці"
        try await makeSession { _ in text }
        session.finish(speech)
        await waitUntil { !self.takes.isEmpty }
        XCTAssertEqual(takes.count, 1)
        XCTAssertTrue(takes[0].raw.contains("ы"))
        XCTAssertFalse(takes[0].cleaned.contains("ы"))
        XCTAssertTrue(takes[0].cleaned.contains("и перевірити"))
        await waitUntil { self.session.state == .idle }
        XCTAssertEqual(session.state, .idle)
        XCTAssertEqual(pastes, 0)
        XCTAssertEqual(formatterCalls.count, 0)
        XCTAssertEqual(try history.count(), 0)
        XCTAssertNil(session.lastRecord)
        XCTAssertEqual(settings.dictionary, ["Shepit"])
    }

    func testCaptureUsesLanguagePolicyHints() async throws {
        try await makeSession { _ in "branch" }
        settings.language = .ukrainian
        session.finish(speech)
        await waitUntil { !self.takes.isEmpty }
        XCTAssertEqual(hints.all.first?.language, "uk")
        XCTAssertEqual(hints.all.first?.vocabulary, ["Shepit"])
    }

    func testCaptureWithoutSpeechDeliversEmptyTake() async throws {
        try await makeSession { _ in "should not be asked" }
        session.finish([Float](repeating: 0, count: 32_000))
        await waitUntil { !self.takes.isEmpty }
        XCTAssertEqual(takes, [CalibrationTake(raw: "", cleaned: "", language: nil)])
        XCTAssertEqual(engine.callCount, 0)
        await waitUntil { self.session.state == .idle }
        XCTAssertEqual(session.state, .idle)
    }

    func testShortCaptureIsDiscarded() async throws {
        try await makeSession { _ in "branch" }
        session.finish(Array(speech.prefix(3_200))) // 0.2 s
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertTrue(takes.isEmpty)
        XCTAssertEqual(engine.callCount, 0)
        XCTAssertEqual(session.state, .idle)
        XCTAssertEqual(pastes, 0)
    }

    func testJunkCleanedToEmpty() async throws {
        try await makeSession { _ in "Thank you for watching" }
        session.finish(speech)
        await waitUntil { !self.takes.isEmpty }
        XCTAssertEqual(takes.first?.raw, "Thank you for watching")
        XCTAssertEqual(takes.first?.cleaned, "")
        XCTAssertEqual(formatterCalls.count, 0)
    }
}
