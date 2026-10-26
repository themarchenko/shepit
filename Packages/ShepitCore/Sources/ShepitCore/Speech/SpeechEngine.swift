import Foundation

public enum EngineID: String, CaseIterable, Codable, Sendable {
    case parakeetV3 = "parakeet-v3"
    case parakeetUltra = "parakeet-ultra"
    case whisperTurbo = "whisper-turbo"

    public var displayName: String {
        switch self {
        case .parakeetV3: return "Parakeet TDT v3 (FluidAudio)"
        case .parakeetUltra: return "Parakeet Ultra (FluidAudio)"
        case .whisperTurbo: return "Whisper large-v3-turbo (WhisperKit)"
        }
    }

    /// Slow enough that long dictations are transcribed in pieces while recording (`ProgressiveTranscriber`).
    public var transcribesProgressively: Bool { self == .whisperTurbo }
}

public struct Transcript: Sendable {
    public let text: String
    /// BCP-47-ish language code reported by the engine, if any ("uk", "en").
    public let language: String?
    public let audioSeconds: Double
    public let processingSeconds: Double
    /// The engine's own confidence in 0...1, where it reports one (Parakeet).
    public let confidence: Float?

    public init(text: String, language: String?, audioSeconds: Double, processingSeconds: Double, confidence: Float? = nil) {
        self.text = text
        self.language = language
        self.audioSeconds = audioSeconds
        self.processingSeconds = processingSeconds
        self.confidence = confidence
    }

    public var isEmpty: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
}

/// What the pipeline knows before recognition starts. Engines use the parts they support.
public struct TranscriptionHints: Sendable {
    /// "uk" or "en" locks the language; nil lets the engine detect it per utterance.
    public var language: String?
    /// Terms the speaker is likely to say (names, identifiers, products). Engines with a text prompt bias toward them.
    public var vocabulary: [String]
    /// Recent text in the speaker's own style (last accepted dictations). Whisper reads it as the preceding segment.
    public var stylePrompt: String?

    public init(language: String? = nil, vocabulary: [String] = [], stylePrompt: String? = nil) {
        self.language = language
        self.vocabulary = vocabulary
        self.stylePrompt = stylePrompt
    }

    public static let none = TranscriptionHints()
}

public enum SpeechError: Error, LocalizedError {
    case notLoaded
    case loadFailed(String)
    case transcriptionFailed(String)

    public var errorDescription: String? {
        switch self {
        case .notLoaded: return "Recognizer is not loaded"
        case .loadFailed(let s): return "Recognizer failed to load: \(s)"
        case .transcriptionFailed(let s): return "Transcription failed: \(s)"
        }
    }
}

/// A speech recognizer that takes 16 kHz mono Float32 samples and returns text.
public protocol SpeechEngine: AnyObject, Sendable {
    var id: EngineID { get }
    var isLoaded: Bool { get async }
    func load() async throws
    func transcribe(_ samples: [Float], hints: TranscriptionHints) async throws -> Transcript
    func unload() async
}

public extension SpeechEngine {
    func transcribe(_ samples: [Float]) async throws -> Transcript {
        try await transcribe(samples, hints: .none)
    }
}

public enum EngineFactory {
    public static func make(_ id: EngineID) -> SpeechEngine {
        switch id {
        case .parakeetV3: return ParakeetEngine(id: id, version: .v3)
        case .parakeetUltra: return ParakeetEngine(id: id, version: .ultra)
        case .whisperTurbo: return WhisperKitEngine(id: id, model: "large-v3-v20240930_turbo")
        }
    }
}

/// Owns the active engine, loads it on demand and swaps it when the setting changes. While a newly
/// chosen engine is still loading (Whisper compiles for minutes on first use) the previous engine keeps
/// serving dictations, so a switch never stalls the user.
///
/// Parakeet v3 also feeds the live preview, so when a slower engine takes over, the Parakeet instance
/// stays loaded next to it instead of being unloaded.
public actor EngineManager {
    private var active: SpeechEngine?
    private var loading: (id: EngineID, task: Task<SpeechEngine, Error>)?
    private(set) public var currentID: EngineID?
    private var preview: SpeechEngine?
    private var previewLoading: Task<Void, Never>?
    private(set) public var keepsPreviewEngine = true

    public init() {}

    public func engine(for id: EngineID) async throws -> SpeechEngine {
        if let active, currentID == id, await active.isLoaded { return active }
        if loading?.id != id { startLoading(id) }
        if let active, let currentID, await active.isLoaded {
            Log.speech.notice("recognizer \(id.rawValue, privacy: .public) still loading, using \(currentID.rawValue, privacy: .public) meanwhile")
            return active
        }
        guard let loading else { throw SpeechError.notLoaded }
        return try await loading.task.value
    }

    public func preload(_ id: EngineID) async {
        if let active, currentID == id, await active.isLoaded { return }
        if loading?.id != id { startLoading(id) }
        _ = try? await loading?.task.value
    }

    public func unloadAll() async {
        loading?.task.cancel()
        loading = nil
        if let active { await active.unload() }
        active = nil
        currentID = nil
        await dropPreview()
    }

    /// The fast engine for the live preview when it is loaded. Starts loading it otherwise and returns nil.
    public func previewEngine() async -> SpeechEngine? {
        guard keepsPreviewEngine else { return nil }
        if currentID == .parakeetV3, let active, await active.isLoaded { return active }
        if let preview, await preview.isLoaded { return preview }
        if previewLoading == nil, loading?.id != .parakeetV3 {
            previewLoading = Task {
                let e = EngineFactory.make(.parakeetV3)
                do {
                    try await e.load()
                    await self.adoptPreview(e)
                } catch {
                    Log.speech.error("preview recognizer failed to load: \(String(describing: error), privacy: .public)")
                    await self.adoptPreview(nil)
                }
            }
        }
        return nil
    }

    public func setKeepsPreviewEngine(_ keep: Bool) async {
        keepsPreviewEngine = keep
        if !keep { await dropPreview() }
    }

    private func adoptPreview(_ e: SpeechEngine?) async {
        previewLoading = nil
        guard let e else { return }
        if keepsPreviewEngine, currentID != .parakeetV3, preview == nil {
            preview = e
        } else {
            await e.unload()
        }
    }

    private func dropPreview() async {
        previewLoading?.cancel()
        previewLoading = nil
        if let preview { await preview.unload() }
        preview = nil
    }

    private func startLoading(_ id: EngineID) {
        loading?.task.cancel()
        let task = Task<SpeechEngine, Error> {
            let e = EngineFactory.make(id)
            Log.speech.notice("loading recognizer \(id.rawValue, privacy: .public)")
            let t0 = Date()
            try await e.load()
            try Task.checkCancellation()
            Log.speech.notice("recognizer \(id.rawValue, privacy: .public) ready in \(Date().timeIntervalSince(t0), format: .fixed(precision: 1)) s")
            await self.adopt(e, id: id)
            return e
        }
        loading = (id, task)
    }

    private func adopt(_ e: SpeechEngine, id: EngineID) async {
        guard loading?.id == id else {
            await e.unload() // a later request superseded this load
            return
        }
        let old = active
        active = e
        currentID = id
        loading = nil
        if id == .parakeetV3 { await dropPreview() }
        if let old, old !== e {
            if keepsPreviewEngine, old.id == .parakeetV3, preview == nil {
                preview = old
            } else {
                await old.unload()
            }
        }
    }
}
