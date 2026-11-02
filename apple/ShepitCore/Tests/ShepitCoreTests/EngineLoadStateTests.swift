import XCTest
@testable import ShepitCore

private struct LoadError: LocalizedError {
    var errorDescription: String? { "disk is full" }
}

private final class Builds: @unchecked Sendable {
    private let lock = NSLock()
    private var n = 0
    /// 1-based index of the engine being built.
    func next() -> Int { lock.withLock { n += 1; return n } }
}

private final class LoadableEngine: SpeechEngine, @unchecked Sendable {
    let id: EngineID
    private let failLoad: Bool
    private let lock = NSLock()
    private var loaded = false

    init(id: EngineID, failLoad: Bool) {
        self.id = id
        self.failLoad = failLoad
    }

    var isLoaded: Bool { get async { lock.withLock { loaded } } }
    func load() async throws {
        if failLoad { throw LoadError() }
        lock.withLock { loaded = true }
    }
    func transcribe(_ samples: [Float], hints: TranscriptionHints) async throws -> Transcript {
        Transcript(text: "", language: nil, audioSeconds: 0, processingSeconds: 0)
    }
    func unload() async { lock.withLock { loaded = false } }
}

final class EngineLoadStateTests: XCTestCase {
    /// The first engine built fails to load, every later one loads.
    private func failingFirst() -> EngineManager {
        let builds = Builds()
        return EngineManager(factory: { id in LoadableEngine(id: id, failLoad: builds.next() == 1) })
    }

    func testUnloadAllClearsTheError() async {
        let m = failingFirst()
        await m.preload(.whisperTurbo)
        await m.unloadAll()
        let state = await m.loadState(of: .whisperTurbo)
        XCTAssertEqual(state, .idle)
    }

    func testIdleBeforeAnyLoad() async {
        let m = failingFirst()
        let state = await m.loadState(of: .whisperTurbo)
        XCTAssertEqual(state, .idle)
    }

    func testLoadedAfterPreload() async {
        let m = EngineManager(factory: { id in LoadableEngine(id: id, failLoad: false) })
        await m.preload(.whisperTurbo)
        let state = await m.loadState(of: .whisperTurbo)
        XCTAssertEqual(state, .loaded)
    }

    func testFailedLoadIsReportedWithTheError() async {
        let m = failingFirst()
        await m.preload(.whisperTurbo)
        let state = await m.loadState(of: .whisperTurbo)
        guard case .failed(let message) = state else { return XCTFail("expected failed, got \(state)") }
        XCTAssertTrue(message.contains("disk is full"), message)
    }

    func testRetryAfterFailureLoadsAgain() async {
        let m = failingFirst()
        await m.preload(.whisperTurbo)
        await m.retry(.whisperTurbo)
        let state = await m.loadState(of: .whisperTurbo)
        XCTAssertEqual(state, .loaded)
    }

    func testRetryClearsTheError() async {
        let m = failingFirst()
        await m.preload(.whisperTurbo)
        await m.retry(.whisperTurbo)
        let state = await m.loadState(of: .whisperTurbo)
        if case .failed = state { XCTFail("error should be cleared after a successful retry") }
    }
}
