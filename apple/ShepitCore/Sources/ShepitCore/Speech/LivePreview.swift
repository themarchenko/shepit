import Foundation

/// What the recorder has captured so far, readable while it keeps recording.
@MainActor
public protocol LiveAudioSource: AnyObject {
    var capturedSampleCount: Int { get }
    func capturedSamples(from start: Int) -> [Float]
}

/// The final recognizer's text for the start of the dictation, as far as it has got (see
/// `ProgressiveTranscriber`): `text` covers the first `samples` samples.
public struct PreviewHead: Equatable, Sendable {
    public var text: String
    public var samples: Int
    public var language: String?

    public init(text: String, samples: Int, language: String?) {
        self.text = text
        self.samples = samples
        self.language = language
    }
}

/// The text shown while the user speaks: the recording so far, recognized again by the fast engine
/// (Parakeet) about twice a second. Throwaway by design; the inserted text still comes from the selected
/// engine after release.
///
/// A pass decodes only the audio after the last freeze point. Once that tail grows past 12 s, its first
/// 8-11 s (cut at the quietest 100 ms, so a word is rarely split) is decoded one last time and frozen.
/// A pass then stays around 0.1-0.3 s on an M1 however long the dictation runs.
///
/// When Whisper has already transcribed the start (`head`), its text replaces everything Parakeet had
/// frozen and Parakeet decodes only after it, so early Parakeet mistakes do not stay on screen. Shown text
/// is also cleaned up: Russian-only letters Parakeet writes for Ukrainian speech are mapped, and the
/// dictionary is applied. None of this touches the inserted text.
@MainActor
public final class LivePreview: ObservableObject {
    public struct Text: Equatable, Sendable {
        /// Words unlikely to change any more.
        public var stable: String
        /// The last words of the current pass; the next pass may still rewrite them.
        public var unstable: String

        public init(stable: String, unstable: String) {
            self.stable = stable
            self.unstable = unstable
        }

        public var isEmpty: Bool { stable.isEmpty && unstable.isEmpty }
        public var full: String { LivePreview.join(stable, unstable) }
        public static let empty = Text(stable: "", unstable: "")
    }

    @Published public private(set) var text = Text.empty
    /// Wall time of the last pass.
    public private(set) var lastPassSeconds: Double = 0
    /// Wall time the dictionary correction took in the last pass, to watch its cost on long dictionaries.
    public private(set) var lastCorrectionSeconds: Double = 0

    private var task: Task<Void, Never>?
    static let sampleRate = 16_000
    static let interval = 0.5
    /// Under a second Parakeet has too little to go on.
    static let minSamples = 16_000
    /// Audio after which the language of the early passes stops being compared.
    static let decideAfter = 3.5

    public init() {}

    /// Starts the passes; `engine` returns nil while the fast engine is still loading.
    ///
    /// Parakeet's language hint is a script filter, and the wrong one turns Ukrainian into Polish-looking
    /// Latin, while no hint flips between Polish, English and Cyrillic in the first seconds. So unless
    /// `hints.language` is a lock, each early pass decodes twice, as Ukrainian and as English, and shows the
    /// winner only when Parakeet is clearly sure of it; after `decideAfter` seconds of audio the winner
    /// sticks. On the user's recordings the more confident script was wrong for 2 of 3 Ukrainian clips at
    /// 1.5 s and right for all 5 clips from 2.5 s. Asking Whisper for the language was right but slow: the
    /// first words came after 4-5 s.
    public func start(source: LiveAudioSource, engine: @escaping @Sendable () async -> SpeechEngine?,
                      hints: TranscriptionHints, head: @escaping @MainActor () -> PreviewHead? = { nil },
                      dictionary: [String] = []) {
        clear()
        task = Task { [weak self] in
            // headText is Whisper's, frozen is Parakeet's since the head, both already corrected.
            var headText = ""
            var headSamples = 0
            var frozen = ""
            var frozenSamples = 0
            var language = hints.language
            while !Task.isCancelled {
                let t0 = Date()
                var correction = 0.0
                // Off the main actor: with a full 300-term dictionary one call took up to 0.13 s.
                func prepare(_ text: String, _ lang: String?) async -> String {
                    let c0 = Date()
                    let out = await Task.detached { Self.prepare(text, language: lang, dictionary: dictionary) }.value
                    correction += Date().timeIntervalSince(c0)
                    return out
                }
                if let h = head(), h.samples > headSamples {
                    headText = await prepare(h.text, h.language)
                    headSamples = h.samples
                    frozen = ""
                    frozenSamples = h.samples
                    if source.capturedSampleCount - frozenSamples < Self.minSamples, !Task.isCancelled, let self {
                        // Too little audio for a pass: show the head alone.
                        self.publish(frozen: Self.join(headText, frozen), current: "")
                        self.lastCorrectionSeconds = correction
                    }
                }
                if source.capturedSampleCount - frozenSamples >= Self.minSamples, let engine = await engine() {
                    var tail = source.capturedSamples(from: frozenSamples)
                    if tail.count > 12 * Self.sampleRate {
                        let cut = Self.quietestCut(in: tail, from: 8 * Self.sampleRate, to: 11 * Self.sampleRate)
                        if let frozenPass = try? await engine.transcribe(Array(tail[..<cut]), hints: Self.hints(hints, language)) {
                            frozen = Self.join(frozen, await prepare(frozenPass.text, language))
                        }
                        frozenSamples += cut
                        tail = Array(tail[cut...])
                    }
                    var pass: Transcript?
                    var passLanguage = language
                    if let language {
                        pass = try? await engine.transcribe(tail, hints: Self.hints(hints, language))
                    } else {
                        let uk = try? await engine.transcribe(tail, hints: Self.hints(hints, "uk"))
                        let en = try? await engine.transcribe(tail, hints: Self.hints(hints, "en"))
                        let ukScore = uk?.confidence ?? 0, enScore = en?.confidence ?? 0
                        let ukWins = ukScore >= enScore
                        if frozenSamples + tail.count >= Int(Self.decideAfter * Double(Self.sampleRate)) {
                            language = ukWins ? "uk" : "en"
                            pass = ukWins ? uk : en
                            passLanguage = language
                        } else if max(ukScore, enScore) >= 0.6, abs(ukScore - enScore) >= 0.2 {
                            pass = ukWins ? uk : en
                            passLanguage = ukWins ? "uk" : "en"
                        }
                    }
                    var current: String?
                    if let pass { current = await prepare(pass.text, passLanguage) }
                    guard !Task.isCancelled, let self else { return }
                    if let current {
                        self.publish(frozen: Self.join(headText, frozen), current: current)
                    }
                    self.lastCorrectionSeconds = correction
                    self.lastPassSeconds = Date().timeIntervalSince(t0)
                }
                let wait = max(0.15, Self.interval - Date().timeIntervalSince(t0))
                try? await Task.sleep(for: .seconds(wait))
            }
        }
    }

    /// Letters first, so a dictionary term written with one of them is not undone. The mapping only runs for
    /// Ukrainian: in English text there is nothing to map and in Russian it would be wrong.
    nonisolated private static func prepare(_ text: String, language: String?, dictionary: [String]) -> String {
        DictionaryCorrector.correct(language == "uk" ? ukrainianLetters(text) : text, dictionary: dictionary)
    }

    /// Parakeet's Cyrillic model also knows Russian, and in a Ukrainian dictation it sometimes writes the
    /// Russian-only letters. Ukrainian has no ы, э, ъ or ё, so each maps to its nearest Ukrainian spelling.
    nonisolated static func ukrainianLetters(_ text: String) -> String {
        let map: [Character: Character] = ["ы": "и", "э": "е", "ъ": "\u{02BC}", "ё": "е",
                                           "Ы": "И", "Э": "Е", "Ъ": "\u{02BC}", "Ё": "Е"]
        return String(text.map { map[$0] ?? $0 })
    }

    private static func hints(_ base: TranscriptionHints, _ language: String?) -> TranscriptionHints {
        var h = base
        h.language = language
        return h
    }

    /// Ends the passes and leaves the last text on screen.
    public func stop() {
        task?.cancel()
        task = nil
    }

    public func clear() {
        stop()
        text = .empty
    }

    private func publish(frozen: String, current: String) {
        let next = Self.split(frozen: frozen, current: current)
        if next != text { text = next }
    }

    /// The last two words of a pass are still likely to change, so they are shown as unstable.
    nonisolated static func split(frozen: String, current: String) -> Text {
        let words = current.split(whereSeparator: \.isWhitespace)
        let unstable = min(2, words.count)
        return Text(stable: join(frozen, words.dropLast(unstable).joined(separator: " ")),
                    unstable: words.suffix(unstable).joined(separator: " "))
    }

    nonisolated static func join(_ a: String, _ b: String) -> String {
        [a, b].filter { !$0.isEmpty }.joined(separator: " ")
    }

    /// Middle of the quietest 100 ms frame between `lower` and `upper`.
    nonisolated static func quietestCut(in samples: [Float], from lower: Int, to upper: Int) -> Int {
        let frame = 1_600
        let end = min(upper, samples.count)
        var best = end
        var bestEnergy = Float.infinity
        var start = lower
        while start + frame <= end {
            var energy: Float = 0
            for i in start..<(start + frame) { energy += samples[i] * samples[i] }
            if energy < bestEnergy {
                bestEnergy = energy
                best = start + frame / 2
            }
            start += frame / 2
        }
        return best
    }
}
