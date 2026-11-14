import XCTest
@testable import ShepitCore

private final class Engine: SpeechEngine, @unchecked Sendable {
    let id: EngineID
    private let lock = NSLock()
    private var loaded = false
    private let gate: AsyncStream<Void>?

    /// With `gate`, `load()` waits until the stream yields, so a test can look at the loading phase.
    init(id: EngineID, gate: AsyncStream<Void>? = nil) {
        self.id = id
        self.gate = gate
    }

    var isLoaded: Bool { get async { lock.withLock { loaded } } }
    func load() async throws {
        if let gate { for await _ in gate { break } }
        lock.withLock { loaded = true }
    }
    func transcribe(_ samples: [Float], hints: TranscriptionHints) async throws -> Transcript {
        Transcript(text: "", language: nil, audioSeconds: 0, processingSeconds: 0)
    }
    func unload() async { lock.withLock { loaded = false } }
}

final class DownloadMeterTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_000)

    func testNoSpeedUntilTwoSecondsOfSamples() {
        var m = DownloadMeter(total: 1_000)
        XCTAssertNil(m.add(bytes: 0, at: t0).bytesPerSecond)
        XCTAssertNil(m.add(bytes: 100, at: t0.addingTimeInterval(1)).secondsLeft)
        let r = m.add(bytes: 200, at: t0.addingTimeInterval(2))
        XCTAssertEqual(r.bytesPerSecond ?? 0, 100, accuracy: 0.001)
        XCTAssertEqual(r.secondsLeft ?? 0, 8, accuracy: 0.001)
    }

    func testSpeedUsesTheLastTenSeconds() {
        var m = DownloadMeter(total: 10_000)
        _ = m.add(bytes: 0, at: t0)
        // Slow start, then 100 B/s: the first sample drops out of the window.
        _ = m.add(bytes: 10, at: t0.addingTimeInterval(5))
        let r = m.add(bytes: 1_010, at: t0.addingTimeInterval(15))
        XCTAssertEqual(r.bytesPerSecond ?? 0, 100, accuracy: 0.001)
    }

    func testRestartedDownloadStartsTheAverageOver() {
        var m = DownloadMeter(total: 1_000)
        _ = m.add(bytes: 500, at: t0)
        _ = m.add(bytes: 600, at: t0.addingTimeInterval(3))
        let r = m.add(bytes: 10, at: t0.addingTimeInterval(4))
        XCTAssertNil(r.bytesPerSecond)
    }

    func testFractionStopsShortOfOne() {
        XCTAssertEqual(DownloadMeter.Reading(bytes: 2_000, total: 1_000).fraction, 0.99)
        XCTAssertEqual(DownloadMeter.Reading(bytes: 250, total: 1_000).fraction, 0.25)
    }

    func testProgressText() {
        let r = DownloadMeter.Reading(bytes: 640_000_000, total: 1_600_000_000, bytesPerSecond: 12_000_000, secondsLeft: 30)
        XCTAssertEqual(ModelStatus.progressText(r), "640 MB of 1.6 GB, 12 MB/s, less than a minute left")
        XCTAssertEqual(ModelStatus.progressText(DownloadMeter.Reading(bytes: 0, total: 1_600_000_000)), "0 bytes of 1.6 GB")
    }

    func testModelFolderBytesCountsPartialFiles() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("a/b"), withIntermediateDirectories: true)
        try Data(count: 100).write(to: dir.appendingPathComponent("a/weight.bin"))
        try Data(count: 50).write(to: dir.appendingPathComponent("a/b/weight.bin.partial"))
        XCTAssertEqual(ModelFolder.bytes(at: dir), 150)
        XCTAssertEqual(ModelFolder.bytes(at: dir.appendingPathComponent("missing")), 0)
    }
}

@MainActor
final class ModelStatusTests: XCTestCase {
    private func status(_ engines: EngineManager, engine: EngineID = .whisperTurbo, bytes: Int64 = 0) -> ModelStatus {
        ModelStatus(engines: engines, engineID: { engine }, measure: { _ in bytes })
    }

    func testIdleBeforeAnyLoad() async {
        let s = status(EngineManager(factory: { Engine(id: $0) }))
        await s.refresh()
        XCTAssertEqual(s.phase, .idle)
        XCTAssertFalse(s.isReady)
        XCTAssertEqual(s.notReadyMessage, "Model is still loading")
    }

    func testReadyAfterLoad() async {
        let engines = EngineManager(factory: { Engine(id: $0) })
        await engines.preload(.whisperTurbo)
        let s = status(engines)
        await s.refresh()
        XCTAssertTrue(s.isReady)
        XCTAssertNil(s.notReadyMessage)
    }

    func testLoadingWithFewBytesIsDownloading() async {
        let (stream, cont) = AsyncStream<Void>.makeStream()
        let engines = EngineManager(factory: { Engine(id: $0, gate: stream) })
        let preload = Task { await engines.preload(.whisperTurbo) }
        try? await Task.sleep(for: .milliseconds(50))
        let s = status(engines, bytes: EngineID.whisperTurbo.downloadBytes / 4)
        await s.refresh()
        guard case .downloading(let r) = s.phase else { return XCTFail("expected downloading, got \(s.phase)") }
        XCTAssertEqual(r.fraction, 0.25, accuracy: 0.01)
        XCTAssertFalse(s.isReady)
        XCTAssertEqual(s.notReadyMessage, "Model downloading: 25%")
        cont.yield()
        cont.finish()
        await preload.value
        await s.refresh()
        XCTAssertTrue(s.isReady)
    }

    func testLoadingWithFilesOnDiskIsPreparing() async {
        let (stream, cont) = AsyncStream<Void>.makeStream()
        let engines = EngineManager(factory: { Engine(id: $0, gate: stream) })
        let preload = Task { await engines.preload(.whisperTurbo) }
        try? await Task.sleep(for: .milliseconds(50))
        let s = status(engines, bytes: EngineID.whisperTurbo.downloadBytes)
        await s.refresh()
        XCTAssertEqual(s.phase, .preparing)
        cont.finish()
        await preload.value
    }

    /// Parakeet loaded and serving does not make a press ready while the chosen Whisper still loads.
    func testFallbackEngineDoesNotCountAsReady() async {
        let (stream, cont) = AsyncStream<Void>.makeStream()
        let engines = EngineManager(factory: { $0 == .parakeetV3 ? Engine(id: $0) : Engine(id: $0, gate: stream) })
        await engines.preload(.parakeetV3)
        let preload = Task { await engines.preload(.whisperTurbo) }
        try? await Task.sleep(for: .milliseconds(50))
        let s = status(engines, bytes: EngineID.whisperTurbo.downloadBytes)
        await s.refresh()
        XCTAssertFalse(s.isReady)
        cont.finish()
        await preload.value
    }

    func testEngineChangeIsNotReadyUntilRefreshed() async {
        let engines = EngineManager(factory: { Engine(id: $0) })
        await engines.preload(.parakeetV3)
        var chosen = EngineID.parakeetV3
        let s = ModelStatus(engines: engines, engineID: { chosen }, measure: { _ in 0 })
        await s.refresh()
        XCTAssertTrue(s.isReady)
        chosen = .whisperTurbo
        XCTAssertFalse(s.isReady)
    }

    func testPressIsRefusedWhileTheModelIsNotReady() async {
        let settings = AppSettings(defaults: UserDefaults(suiteName: UUID().uuidString)!)
        settings.engine = .whisperTurbo
        let (stream, cont) = AsyncStream<Void>.makeStream()
        let engines = EngineManager(factory: { Engine(id: $0, gate: stream) })
        let session = DictationSession(recorder: AudioRecorder(), engines: engines,
                                       formatting: FormattingService(claude: nil), inserter: PasteInserter(),
                                       history: nil, settings: settings)
        let preload = Task { await engines.preload(.whisperTurbo) }
        try? await Task.sleep(for: .milliseconds(50))
        await session.modelStatus.refresh()
        session.startRecording(handsFree: false)
        guard case .failed(let message) = session.state else { return XCTFail("expected refusal, got \(session.state)") }
        XCTAssertTrue(message.hasPrefix("Model "), message)
        XCTAssertFalse(session.recorder.isRecording)
        cont.finish()
        await preload.value
    }
}
