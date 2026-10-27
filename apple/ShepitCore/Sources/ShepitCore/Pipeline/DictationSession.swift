import Foundation

/// The dictation state machine: record, transcribe, format, insert, log.
@MainActor
public final class DictationSession: ObservableObject {
    public enum State: Equatable, Sendable {
        case idle
        case recording(handsFree: Bool)
        case loadingModel
        case transcribing
        case formatting
        case inserting
        case done(preview: String)
        case failed(message: String)

        public var isRecording: Bool {
            if case .recording = self { return true }
            return false
        }

        public var isBusy: Bool {
            switch self {
            case .loadingModel, .transcribing, .formatting, .inserting: return true
            default: return false
            }
        }
    }

    @Published public private(set) var state: State = .idle
    /// Mode requested for the next dictation; nil means the setting's default.
    @Published public var modeOverride: Mode?
    @Published public private(set) var lastRecord: DictationRecord?
    /// The words so far while recording; kept through processing, cleared once the result is in.
    public let live = LivePreview()
    private let progressive = ProgressiveTranscriber()
    private var recognitionHints = TranscriptionHints()

    public let recorder: AudioRecorder
    private let engines: EngineManager
    private let formatting: FormattingService
    private let inserter: PasteInserter
    private let history: HistoryStore?
    private let settings: AppSettings

    private var context = TargetContext()
    private var recordingStart: Date?
    private var processTask: Task<Void, Never>?
    private var resetTask: Task<Void, Never>?
    public var minimumSeconds: Double = 0.4

    public init(recorder: AudioRecorder, engines: EngineManager, formatting: FormattingService,
                inserter: PasteInserter, history: HistoryStore?, settings: AppSettings) {
        self.recorder = recorder
        self.engines = engines
        self.formatting = formatting
        self.inserter = inserter
        self.history = history
        self.settings = settings
    }

    /// The app that was focused when the current dictation started.
    public var targetAppName: String? { context.appName }

    // MARK: Controls

    public func startRecording(handsFree: Bool) {
        switch state {
        case .idle, .done, .failed: break
        default: return
        }
        resetTask?.cancel()
        context = ContextProbe.capture()
        do {
            try recorder.start()
        } catch {
            fail("Microphone: \(error.localizedDescription)")
            return
        }
        recordingStart = Date()
        state = .recording(handsFree: handsFree)
        // Whisper decodes its prompt token by token, so a long dictionary slows every dictation: it gets the
        // 30 newest terms only. Recent dictations are not used as a prompt: Whisper followed their language
        // and wrote English speech in Ukrainian ("Okay, let's test this app" came out "Окей, давайте тестимемо
        // цю апп").
        recognitionHints = TranscriptionHints(language: settings.languageLock == "auto" ? nil : settings.languageLock,
                                              vocabulary: Array(settings.dictionary.suffix(30)))
        let engines = self.engines
        let engineID = settings.engine
        progressive.start(source: recorder, engine: { try? await engines.engine(for: engineID) },
                          hints: recognitionHints)
        if settings.livePreview {
            live.start(source: recorder, engine: { await engines.previewEngine() },
                       hints: TranscriptionHints(language: settings.languageLock == "auto" ? nil : settings.languageLock))
        } else {
            live.clear()
        }
        Log.pipeline.notice("recording started for \(self.context.appName ?? "?", privacy: .public) (hands-free: \(handsFree, privacy: .public))")
    }

    /// Keeps the current recording going without the key held (after a double tap).
    public func continueHandsFree() {
        if case .recording = state { state = .recording(handsFree: true) }
    }

    public func stopRecording() {
        guard case .recording = state else { return }
        live.stop()
        let samples = recorder.stop()
        let seconds = Double(samples.count) / 16_000
        guard seconds >= minimumSeconds else {
            Log.pipeline.debug("discarding \(seconds, format: .fixed(precision: 2)) s recording")
            live.clear()
            progressive.cancel()
            state = .idle
            return
        }
        processTask = Task { await process(samples) }
    }

    public func cancel() {
        if case .recording = state { _ = recorder.stop() }
        live.clear()
        progressive.cancel()
        processTask?.cancel()
        processTask = nil
        state = .idle
    }

    public func toggleHandsFree() {
        switch state {
        case .recording(handsFree: true): stopRecording()
        case .recording(handsFree: false): continueHandsFree()
        case .idle, .done, .failed: startRecording(handsFree: true)
        default: break
        }
    }

    // MARK: Pipeline

    private func process(_ samples: [Float]) async {
        let audioSeconds = Double(samples.count) / 16_000
        // Keep the audio before anything can fail: failed recognitions are the recordings worth replaying.
        let audioFile = RecordingStore.fileName()
        Task.detached(priority: .utility) {
            do {
                try RecordingStore.save(samples, as: audioFile)
                RecordingStore.prune()
            } catch {
                Log.store.error("could not save recording: \(String(describing: error), privacy: .public)")
            }
        }
        let engineID = settings.engine
        var sttMs = 0
        var llmMs = 0
        let speechSeconds = SpeechPresence.speechSeconds(in: samples)
        guard speechSeconds >= 0.3 else {
            Log.pipeline.notice("no speech in \(audioSeconds, format: .fixed(precision: 1)) s of audio, skipping recognition")
            fail("Nothing recognized")
            return
        }
        do {
            let engine: SpeechEngine
            if let current = await engines.currentID, current == engineID {
                state = .transcribing
                engine = try await engines.engine(for: engineID)
            } else {
                state = .loadingModel
                engine = try await engines.engine(for: engineID)
                state = .transcribing
            }
            let t0 = Date()
            let transcript: Transcript
            let usedEngine: EngineID
            if let early = await progressive.complete(samples) {
                (transcript, usedEngine) = early
            } else {
                transcript = try await engine.transcribe(samples, hints: recognitionHints)
                usedEngine = engine.id
            }
            sttMs = Int(Date().timeIntervalSince(t0) * 1000)
            try Task.checkCancellation()
            guard !transcript.isEmpty, !HallucinationFilter.isJunk(transcript.text) else {
                fail("Nothing recognized")
                return
            }
            Log.pipeline.notice("stt \(sttMs, privacy: .public) ms: \(String(transcript.text.prefix(80)), privacy: .private)")

            state = .formatting
            let profile = ProfileResolver.resolve(context)
            let requested = modeOverride ?? settings.defaultMode
            let mode = ModeResolver.resolve(requested: requested, profile: profile,
                                            wordCount: ModeResolver.wordCount(transcript.text),
                                            promptModeMinWords: settings.promptModeMinWords)
            let languageHint = settings.languageLock == "auto" ? transcript.language : settings.languageLock
            // The context was captured at record start; a password field focused since then must not reach the agent either.
            if ContextProbe.isSecureInputActive { context.secureInput = true }
            let request = FormatRequest(transcript: transcript.text, languageHint: languageHint, mode: mode,
                                        profile: profile, context: context, dictionary: settings.dictionary)
            let t1 = Date()
            let result = await formatting.format(request)
            llmMs = Int(Date().timeIntervalSince(t1) * 1000)
            try Task.checkCancellation()
            Log.pipeline.notice("format \(llmMs, privacy: .public) ms via \(result.source.rawValue, privacy: .public)")
            let learned = DictionaryLearning.newEntries(from: result.newTerms, existing: settings.dictionary)
            if !learned.isEmpty {
                settings.dictionary += learned
                Log.pipeline.notice("dictionary learned \(learned.joined(separator: ", "), privacy: .private)")
            }

            state = .inserting
            let method = try await inserter.insert(result.text, context: context)
            let record = DictationRecord(
                bundleID: context.bundleID, appName: context.appName, windowTitle: context.windowTitle,
                profile: profile.id, mode: mode.rawValue, engine: usedEngine.rawValue,
                rawText: transcript.text, formattedText: result.text, language: result.language ?? transcript.language,
                audioSeconds: audioSeconds, sttMs: sttMs, llmMs: llmMs, formatterSource: result.source.rawValue,
                insertMethod: method.rawValue, status: method == .clipboardOnly ? "clipboard" : "inserted",
                audioFile: audioFile)
            lastRecord = (try? history?.save(record)) ?? record
            let preview = String(result.text.prefix(60)).replacingOccurrences(of: "\n", with: " ")
            live.clear()
            state = .done(preview: method == .clipboardOnly ? "Copied (grant Accessibility to paste): \(preview)" : preview)
            scheduleReset(after: 2.5)
        } catch is CancellationError {
            state = .idle
        } catch {
            fail(error.localizedDescription)
        }
    }

    private func fail(_ message: String) {
        live.clear()
        progressive.cancel()
        Log.pipeline.error("dictation failed: \(message, privacy: .public)")
        state = .failed(message: message)
        scheduleReset(after: 4)
    }

    private func scheduleReset(after seconds: Double) {
        resetTask?.cancel()
        resetTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            guard let self, !Task.isCancelled else { return }
            switch self.state {
            case .done, .failed: self.state = .idle
            default: break
            }
        }
    }
}
