import Foundation

/// What the recorder has captured so far, readable while it keeps recording.
@MainActor
public protocol LiveAudioSource: AnyObject {
    var capturedSampleCount: Int { get }
    func capturedSamples(from start: Int) -> [Float]
}

/// The text shown while the user speaks: the recording so far, recognized again by the fast engine
/// (Parakeet) about twice a second. Throwaway by design; the inserted text still comes from the selected
/// engine after release.
///
/// A pass decodes only the audio after the last freeze point. Once that tail grows past 12 s, its first
/// 8-11 s (cut at the quietest 100 ms, so a word is rarely split) is decoded one last time and frozen.
/// A pass then stays around 0.1-0.3 s on an M1 however long the dictation runs.
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
                      hints: TranscriptionHints) {
        clear()
        task = Task { [weak self] in
            var frozen = ""
            var frozenSamples = 0
            var language = hints.language
            while !Task.isCancelled {
                let t0 = Date()
                if source.capturedSampleCount - frozenSamples >= Self.minSamples, let engine = await engine() {
                    var tail = source.capturedSamples(from: frozenSamples)
                    if tail.count > 12 * Self.sampleRate {
                        let cut = Self.quietestCut(in: tail, from: 8 * Self.sampleRate, to: 11 * Self.sampleRate)
                        if let head = try? await engine.transcribe(Array(tail[..<cut]), hints: Self.hints(hints, language)) {
                            frozen = Self.join(frozen, head.text)
                        }
                        frozenSamples += cut
                        tail = Array(tail[cut...])
                    }
                    var pass: Transcript?
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
                        } else if max(ukScore, enScore) >= 0.6, abs(ukScore - enScore) >= 0.2 {
                            pass = ukWins ? uk : en
                        }
                    }
                    guard !Task.isCancelled, let self else { return }
                    if let pass { self.publish(frozen: frozen, current: pass.text) }
                    self.lastPassSeconds = Date().timeIntervalSince(t0)
                }
                let wait = max(0.15, Self.interval - Date().timeIntervalSince(t0))
                try? await Task.sleep(for: .seconds(wait))
            }
        }
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
