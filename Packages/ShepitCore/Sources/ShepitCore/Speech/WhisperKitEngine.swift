import Foundation
import WhisperKit

/// OpenAI Whisper through WhisperKit (CoreML). Slower than Parakeet, broader language coverage, per-utterance language id.
public actor WhisperKitEngine: SpeechEngine {
    public let id: EngineID
    private let model: String
    private var kit: WhisperKit?

    public init(id: EngineID, model: String) {
        self.id = id
        self.model = model
    }

    public var isLoaded: Bool { kit != nil }

    public func load() async throws {
        guard kit == nil else { return }
        let base = AppPaths.modelsDirectory.appendingPathComponent("whisperkit", isDirectory: true)
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        do {
            let config = WhisperKitConfig(model: model, downloadBase: base, verbose: false, logLevel: .error,
                                          prewarm: true, load: true, download: true)
            kit = try await WhisperKit(config)
        } catch {
            throw SpeechError.loadFailed(String(describing: error))
        }
    }

    public func transcribe(_ samples: [Float], hints: TranscriptionHints) async throws -> Transcript {
        guard let kit else { throw SpeechError.notLoaded }
        let t0 = Date()
        do {
            let prompt = promptTokens(for: hints, kit: kit)
            var language = hints.language
            var results = try await kit.transcribe(audioArray: samples, decodeOptions: options(language: language, prompt: prompt))
            if language == nil {
                let detected = results.first?.language
                language = detected
                if let retry = Self.retryLanguage(detected: detected, text: Self.joined(results)) {
                    Log.speech.notice("whisper detected \(detected ?? "nil", privacy: .public), retrying as \(retry, privacy: .public)")
                    results = try await kit.transcribe(audioArray: samples, decodeOptions: options(language: retry, prompt: prompt))
                    language = retry
                }
            }
            return Transcript(text: Self.joined(results), language: language,
                              audioSeconds: Double(samples.count) / 16_000,
                              processingSeconds: Date().timeIntervalSince(t0))
        } catch {
            throw SpeechError.transcriptionFailed(String(describing: error))
        }
    }

    /// The speaker uses Ukrainian and English only, but Whisper's language id picks from about 100 languages and
    /// hears accented Ukrainian as Russian now and then; the decoder then writes Russian words. Returns the
    /// language to decode again with, chosen by the script of the first pass, or nil when no retry is needed.
    static func retryLanguage(detected: String?, text: String) -> String? {
        guard let detected, !["uk", "en"].contains(detected) else { return nil }
        let cyrillic = text.unicodeScalars.contains { (0x0400...0x04FF).contains($0.value) }
        return cyrillic ? "uk" : "en"
    }

    private static func joined(_ results: [TranscriptionResult]) -> String {
        results.map { $0.text.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }.joined(separator: " ")
    }

    private func options(language: String?, prompt: [Int]?) -> DecodingOptions {
        DecodingOptions(task: .transcribe, language: language,
                        usePrefillPrompt: true, detectLanguage: language == nil,
                        skipSpecialTokens: true, withoutTimestamps: true,
                        promptTokens: prompt)
    }

    public func unload() async {
        kit = nil
    }

    /// Whisper reads the prompt as text that preceded the audio: recent dictations set the style (Ukrainian with
    /// Latin identifiers) and a comma-separated term list nudges spellings. The terms go last because WhisperKit
    /// keeps the tail of an over-long prompt; it also drops special tokens.
    private func promptTokens(for hints: TranscriptionHints, kit: WhisperKit) -> [Int]? {
        guard let tokenizer = kit.tokenizer else { return nil }
        var parts: [String] = []
        if let style = hints.stylePrompt?.trimmingCharacters(in: .whitespacesAndNewlines), !style.isEmpty {
            parts.append(String(style.suffix(400)))
        }
        if !hints.vocabulary.isEmpty { parts.append(hints.vocabulary.joined(separator: ", ")) }
        guard !parts.isEmpty else { return nil }
        let tokens = tokenizer.encode(text: " " + parts.joined(separator: " "))
            .filter { $0 < tokenizer.specialTokens.specialTokenBegin }
        return tokens.isEmpty ? nil : tokens
    }
}
