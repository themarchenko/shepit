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
            var state = try TdtDecoderState(decoderLayers: await manager.decoderLayerCount)
            // The hint filters decoder candidates by script, so only a hard language lock may set it:
            // with mixed Ukrainian and English speech it would force identifiers into Cyrillic.
            let result = try await manager.transcribe(samples, decoderState: &state, language: Self.language(for: hints.language))
            return Transcript(text: result.text, language: nil,
                              audioSeconds: Double(samples.count) / 16_000,
                              processingSeconds: Date().timeIntervalSince(t0), confidence: result.confidence)
        } catch {
            throw SpeechError.transcriptionFailed(String(describing: error))
        }
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
