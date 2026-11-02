import Foundation
import CoreML
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

    /// WhisperKit keeps a model under `models/argmaxinc/whisperkit-coreml/openai_whisper-<model>`.
    nonisolated static func modelFolderPresent(base: URL, model: String) -> Bool {
        let folder = base.appendingPathComponent("models/argmaxinc/whisperkit-coreml/openai_whisper-\(model)", isDirectory: true)
        return ["AudioEncoder", "TextDecoder", "MelSpectrogram"].allSatisfy {
            FileManager.default.fileExists(atPath: folder.appendingPathComponent("\($0).mlmodelc").path)
        }
    }

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
                // Real uk/en probabilities cost one extra encoder pass, so only when detection left the set
                // and the first pass is not Cyrillic (that case retries as Ukrainian without them).
                let probabilities = Self.needsProbe(detected: detected, text: Self.joined(results), policy: hints.policy)
                    ? (try? await languageProbabilities(samples, kit: kit)) ?? nil : nil
                // A cancelled dictation must not pay for the retry decode.
                try Task.checkCancellation()
                if let retry = Self.retryLanguage(detected: detected, text: Self.joined(results),
                                                  probabilities: probabilities, policy: hints.policy) {
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
        return scriptLanguage(text)
    }

    private static func scriptLanguage(_ text: String) -> String {
        let cyrillic = text.unicodeScalars.contains { (0x0400...0x04FF).contains($0.value) }
        return cyrillic ? "uk" : "en"
    }

    /// Same, for a policy. A Cyrillic first pass retries as Ukrainian: Whisper's uk/en logits lean English, and
    /// on a real recording heard as Russian they picked English, which translated the Ukrainian speech. For a
    /// Latin first pass (Polish and the like) the allowed language Whisper finds more likely wins; without
    /// probabilities the script decides. The result is always inside the policy.
    static func retryLanguage(detected: String?, text: String, probabilities: [String: Float]?,
                              policy: LanguagePolicy) -> String? {
        guard let detected, !policy.isAllowed(detected) else { return nil }
        let script = scriptLanguage(text)
        let chosen = script == "uk" ? script
            : probabilities.flatMap(policy.pick(fromProbabilities:)) ?? script
        return policy.isAllowed(chosen) ? chosen : policy.allowed[0]
    }

    /// Whether the uk/en probe is worth its extra encoder pass: only when detection left the set and the first
    /// pass is not Cyrillic.
    static func needsProbe(detected: String?, text: String, policy: LanguagePolicy) -> Bool {
        guard let detected, !policy.isAllowed(detected) else { return false }
        return scriptLanguage(text) != "uk"
    }

    /// Softmax over the captured logits. Both uk and en must be present: one alone would read as certain,
    /// so nil sends the caller to the script rule.
    static func languageProbabilities(fromLogits logits: [String: Float]) -> [String: Float]? {
        guard logits["uk"] != nil, logits["en"] != nil, let top = logits.values.max() else { return nil }
        let exps = logits.mapValues { Foundation.exp($0 - top) }
        let sum = exps.values.reduce(0, +)
        return exps.mapValues { $0 / sum }
    }

    /// WhisperKit's own language result keeps only the winner's probability, so this repeats its detection
    /// step (WhisperKit.detectLangauge) with a sampler that reads the Ukrainian and English logits.
    private func languageProbabilities(_ samples: [Float], kit: WhisperKit) async throws -> [String: Float]? {
        guard let tokenizer = kit.tokenizer,
              let uk = tokenizer.convertTokenToId("<|uk|>"), let en = tokenizer.convertTokenToId("<|en|>") else { return nil }
        let options = DecodingOptions()
        let inputs = try kit.textDecoder.prepareDecoderInputs(withPrompt: [tokenizer.specialTokens.startOfTranscriptToken])
        guard let window = kit.audioProcessor.padOrTrim(fromArray: samples, startAt: 0,
                                                        toLength: kit.featureExtractor.windowSamples ?? Constants.defaultWindowSamples),
              let mel = try await kit.featureExtractor.logMelSpectrogram(fromAudio: window),
              let encoded = try await kit.audioEncoder.encodeFeatures(mel) else { return nil }
        let capture = LanguageLogitCapture(
            tokens: ["uk": uk, "en": en],
            inner: GreedyTokenSampler(temperature: 0, eotToken: tokenizer.specialTokens.endToken, decodingOptions: options))
        _ = try await kit.textDecoder.detectLanguage(from: encoded, using: inputs, sampler: capture, options: options, temperature: 0)
        return Self.languageProbabilities(fromLogits: capture.logits)
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

/// Records the logits of chosen tokens on the language-detection step and leaves the sampling to the inner sampler.
private final class LanguageLogitCapture: TokenSampling, @unchecked Sendable {
    private let tokens: [String: Int]
    private let inner: TokenSampling
    private(set) var logits: [String: Float] = [:]

    init(tokens: [String: Int], inner: TokenSampling) {
        self.tokens = tokens
        self.inner = inner
    }

    func update(tokens current: [Int], logits array: MLMultiArray, logProbs: [Float]) async -> SamplingResult {
        for (code, token) in tokens where token < array.count {
            logits[code] = array[token].floatValue
        }
        return await inner.update(tokens: current, logits: array, logProbs: logProbs)
    }

    func finalize(tokens: [Int], logProbs: [Float]) -> SamplingResult {
        inner.finalize(tokens: tokens, logProbs: logProbs)
    }
}
