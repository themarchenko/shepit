import FluidAudio
import Foundation

/// NVIDIA Parakeet TDT models through FluidAudio (CoreML, on-device, 25 European languages including Ukrainian).
public actor ParakeetEngine: SpeechEngine {
    public let id: EngineID
    private let version: AsrModelVersion
    private var manager: AsrManager?

    public init(id: EngineID, version: AsrModelVersion) {
        self.id = id
        self.version = version
    }

    public var isLoaded: Bool { manager != nil }

    public func load() async throws {
        guard manager == nil else { return }
        do {
            let models = try await AsrModels.downloadAndLoad(version: version)
            let m = AsrManager(config: .default)
            try await m.loadModels(models)
            manager = m
        } catch {
            throw SpeechError.loadFailed(String(describing: error))
        }
    }

    public func transcribe(_ samples: [Float], hints: TranscriptionHints) async throws -> Transcript {
        guard let manager else { throw SpeechError.notLoaded }
        let t0 = Date()
        do {
            // The hint filters decoder candidates by script, so only a hard language lock may set it:
            // with mixed Ukrainian and English speech it would force identifiers into Cyrillic.
            let first = try await decode(samples, language: Self.language(for: hints.language), manager: manager)
            let resolved = await ScriptCleanup.resolvePolishLeak(first: first, policy: hints.policy) { code in
                try await self.decode(samples, language: Self.language(for: code), manager: manager)
            }
            return Transcript(text: resolved.text, language: nil,
                              audioSeconds: Double(samples.count) / 16_000,
                              processingSeconds: Date().timeIntervalSince(t0), confidence: resolved.confidence)
        } catch {
            throw SpeechError.transcriptionFailed(String(describing: error))
        }
    }

    /// Each decode starts from a fresh decoder state, so a re-decode does not continue from the first pass.
    private func decode(_ samples: [Float], language: Language?, manager: AsrManager) async throws -> Transcript {
        var state = try TdtDecoderState(decoderLayers: await manager.decoderLayerCount)
        let result = try await manager.transcribe(samples, decoderState: &state, language: language)
        return Transcript(text: result.text, language: nil, audioSeconds: Double(samples.count) / 16_000,
                          processingSeconds: 0, confidence: result.confidence)
    }

    public func unload() async {
        await manager?.cleanup()
        manager = nil
    }

    private static func language(for code: String?) -> Language? {
        switch code {
        case "uk": return .ukrainian
        case "en": return .english
        default: return nil
        }
    }
}
