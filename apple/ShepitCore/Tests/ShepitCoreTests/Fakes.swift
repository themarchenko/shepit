import Foundation
import XCTest
@testable import ShepitCore

/// A recording that exists only as a sample count and a generator, so tests can grow it on demand.
/// The default generator makes `samples[i] == Float(i)`, which lets a fake engine report which slice it
/// was given (see `positionText`).
@MainActor
final class FakeLiveSource: LiveAudioSource {
    var count: Int
    private let generator: @Sendable (Int) -> Float

    init(count: Int = 0, generator: @escaping @Sendable (Int) -> Float = { Float($0) }) {
        self.count = count
        self.generator = generator
    }

    var capturedSampleCount: Int { count }

    func capturedSamples(from start: Int) -> [Float] {
        guard start < count else { return [] }
        return (start..<count).map(generator)
    }

    /// A sine that alternates loud and near-silent 0.2 s blocks. `SpeechPresence` measures speech
    /// against the noise floor of the same audio, so a constant tone counts as no speech at all.
    static let speech: @Sendable (Int) -> Float = { i in
        (i / 3_200) % 2 == 0 ? 0.3 * sinf(Float(i) * 0.05) : 0.002 * sinf(Float(i) * 0.05)
    }

    /// Speech for the first `seconds`, silence after.
    static func speech(forSeconds seconds: Double) -> @Sendable (Int) -> Float {
        let limit = Int(seconds * 16_000)
        return { $0 < limit ? speech($0) : 0 }
    }
}

final class FakeEngine: SpeechEngine, @unchecked Sendable {
    typealias Responder = @Sendable ([Float], TranscriptionHints) throws -> Transcript

    let id: EngineID
    private let responder: Responder
    private let lock = NSLock()
    private var calls = 0

    init(id: EngineID, responder: @escaping Responder) {
        self.id = id
        self.responder = responder
    }

    var callCount: Int {
        lock.withLock { calls }
    }

    var isLoaded: Bool { get async { true } }
    func load() async throws {}
    func unload() async {}

    private func recordCall() {
        lock.withLock { calls += 1 }
    }

    func transcribe(_ samples: [Float], hints: TranscriptionHints) async throws -> Transcript {
        recordCall()
        return try responder(samples, hints)
    }
}

func fakeTranscript(_ text: String, language: String? = nil, confidence: Float? = nil) -> Transcript {
    Transcript(text: text, language: language, audioSeconds: 0, processingSeconds: 0, confidence: confidence)
}

/// "p<first>_<last+1>" for a slice of a `FakeLiveSource` with the default generator.
func positionText(_ samples: [Float]) -> String {
    guard let first = samples.first, let last = samples.last else { return "" }
    return "p\(Int(first))_\(Int(last) + 1)"
}

/// Polls instead of sleeping a fixed time, so the tests do not depend on the pass cadence.
@MainActor
@discardableResult
func waitUntil(timeout: TimeInterval = 5, poll: TimeInterval = 0.05, _ condition: () -> Bool) async -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if condition() { return true }
        try? await Task.sleep(for: .seconds(poll))
    }
    return condition()
}
