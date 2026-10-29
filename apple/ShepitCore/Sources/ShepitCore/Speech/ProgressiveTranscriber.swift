import Foundation

/// Transcribes a long dictation with the final recognizer while it is still being recorded, so only the
/// last piece is left after release. Whisper turbo needs about a quarter of the audio's length on an M1
/// (10.8 s for a 46 s dictation); here it gets the audio in 7-12 s pieces cut at quiet points, and each
/// piece is done well before the next one is complete.
///
/// Later pieces get the text so far as their prompt (Whisper's usual conditioning on the previous
/// segment) and the first piece's language, so a dictation does not switch language halfway.
@MainActor
public final class ProgressiveTranscriber {
    /// A piece is cut once this much audio is waiting.
    static let pieceTrigger = 14 * 16_000
    static let cutWindow = (lower: 7 * 16_000, upper: 12 * 16_000)

    /// The text so far and the sample it ends at, for the live preview to show instead of its own early text.
    public private(set) var head: PreviewHead?

    private var task: Task<Void, Never>?
    private var engine: SpeechEngine?
    private var hints = TranscriptionHints()
    private var text = ""
    private var language: String?
    private var doneSamples = 0
    private var failed = false
    private var inFlight = false
    private var stopping = false

    public init() {}

    /// Starts watching the recording. `engine` is asked once, when the first piece is due; an engine that
    /// is fast enough on its own (Parakeet, also the stand-in while Whisper loads) turns this off.
    public func start(source: LiveAudioSource, engine provider: @escaping @Sendable () async -> SpeechEngine?,
                      hints: TranscriptionHints) {
        cancel()
        self.hints = hints
        engine = nil
        text = ""
        language = nil
        doneSamples = 0
        head = nil
        failed = false
        inFlight = false
        stopping = false
        task = Task { [weak self] in
            while true {
                guard let self, !self.stopping, !Task.isCancelled else { return }
                guard source.capturedSampleCount - self.doneSamples >= Self.pieceTrigger else {
                    try? await Task.sleep(for: .seconds(0.5))
                    continue
                }
                if self.engine == nil {
                    guard let e = await provider(), e.id.transcribesProgressively else { return }
                    self.engine = e
                }
                guard !self.stopping, !Task.isCancelled, let engine = self.engine else { return }
                let pending = source.capturedSamples(from: self.doneSamples)
                let cut = LivePreview.quietestCut(in: pending, from: Self.cutWindow.lower, to: Self.cutWindow.upper)
                self.inFlight = true
                let ok = await self.transcribe(Array(pending[..<cut]), with: engine)
                self.inFlight = false
                guard ok else {
                    self.failed = true
                    return
                }
                self.doneSamples += cut
                // Set in the same step as doneSamples so the preview never sees text and end sample apart.
                self.head = PreviewHead(text: self.text, samples: self.doneSamples,
                                        language: self.hints.language ?? self.language)
            }
        }
    }

    /// Finishes after release: waits for the piece in flight, transcribes the rest of `samples` and returns
    /// the whole transcript. nil when nothing was transcribed early (a short dictation, a fast engine, a
    /// failed piece); the caller then transcribes the recording as usual.
    public func complete(_ samples: [Float]) async -> (transcript: Transcript, engine: EngineID)? {
        stopping = true
        if !inFlight { task?.cancel() }
        await task?.value
        task = nil
        guard !failed, doneSamples > 0, doneSamples <= samples.count, let engine else { return nil }
        let t0 = Date()
        guard await transcribe(Array(samples[doneSamples...]), with: engine) else { return nil }
        Log.speech.notice("progressive: \(Double(self.doneSamples) / 16_000, format: .fixed(precision: 1)) s done early, \(Double(samples.count - self.doneSamples) / 16_000, format: .fixed(precision: 1)) s after release")
        let transcript = Transcript(text: text, language: hints.language ?? language,
                                    audioSeconds: Double(samples.count) / 16_000,
                                    processingSeconds: Date().timeIntervalSince(t0))
        return (transcript, engine.id)
    }

    public func cancel() {
        stopping = true
        task?.cancel()
        task = nil
    }

    /// Returns false when the recognizer failed.
    private func transcribe(_ piece: [Float], with engine: SpeechEngine) async -> Bool {
        guard SpeechPresence.speechSeconds(in: piece) >= 0.3 else { return true }
        var h = hints
        if h.language == nil { h.language = language }
        if !text.isEmpty { h.stylePrompt = String(text.suffix(400)) }
        do {
            let t = try await engine.transcribe(piece, hints: h)
            if language == nil, let l = t.language, ["uk", "en"].contains(l) { language = l }
            if !HallucinationFilter.isJunk(t.text) { text = LivePreview.join(text, t.text) }
            return true
        } catch {
            Log.speech.error("progressive piece failed: \(String(describing: error), privacy: .public)")
            return false
        }
    }
}
