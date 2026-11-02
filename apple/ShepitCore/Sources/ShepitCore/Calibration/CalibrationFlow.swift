import Foundation

/// The wizard's steps as a value type, so the order of stages and the pairs sent to the agent are testable
/// without a window. The model in the app owns one and mutates it.
public struct CalibrationFlow: Equatable {
    public enum Section: Equatable, Sendable { case words, phrases, targeted }
    public enum Stage: Equatable, Sendable {
        case intro
        case reading(Section, Int)
        case pickingSentences
        case writingNotes
        case review
        case failed
    }

    public private(set) var stage: Stage = .intro
    private var policy: LanguagePolicy = .ukrainianEnglish
    private var targeted: [CalibrationPrompt] = []
    /// Keyed by prompt id, so going back and forth keeps what was read.
    private var takes: [String: CalibrationTake] = [:]

    public init() {}

    private func prompts(_ section: Section) -> [CalibrationPrompt] {
        switch section {
        case .words: return CalibrationScript.words(for: policy)
        case .phrases: return CalibrationScript.phrases(for: policy)
        case .targeted: return targeted
        }
    }

    public var currentPrompt: CalibrationPrompt? {
        guard case let .reading(section, index) = stage else { return nil }
        let list = prompts(section)
        return list.indices.contains(index) ? list[index] : nil
    }

    public var currentTake: CalibrationTake? {
        currentPrompt.flatMap { takes[$0.id] }
    }

    public mutating func start(policy: LanguagePolicy) {
        self.policy = policy
        takes = [:]
        targeted = []
        stage = .reading(.words, 0)
    }

    public mutating func record(_ take: CalibrationTake) {
        guard let prompt = currentPrompt else { return }
        takes[prompt.id] = take
    }

    /// A take belongs to the prompt that was on screen when the key went down. One that finishes after the user
    /// moved on is dropped instead of landing on the next prompt.
    public mutating func record(_ take: CalibrationTake, for promptID: String) {
        guard currentPrompt?.id == promptID else { return }
        record(take)
    }

    public mutating func again() {
        guard let prompt = currentPrompt else { return }
        takes[prompt.id] = nil
    }

    public mutating func next() {
        guard case let .reading(section, index) = stage else { return }
        if index + 1 < prompts(section).count {
            stage = .reading(section, index + 1)
            return
        }
        switch section {
        case .words: stage = .reading(.phrases, 0)
        case .phrases: stage = .pickingSentences
        case .targeted: stage = .writingNotes
        }
    }

    public mutating func back() {
        guard case let .reading(section, index) = stage else { return }
        if index > 0 {
            stage = .reading(section, index - 1)
            return
        }
        switch section {
        case .words: stage = .intro
        case .phrases: stage = .reading(.words, max(0, prompts(.words).count - 1))
        case .targeted: break // the sentences were picked from the earlier takes; going back would re-pick them
        }
    }

    public mutating func setTargeted(_ prompts: [CalibrationPrompt]) {
        targeted = prompts
        stage = .reading(.targeted, 0)
    }

    public mutating func profileReady() { stage = .review }
    public mutating func profileFailed() { stage = .failed }
    public mutating func retry() { stage = .writingNotes }

    /// Recorded prompts only, in the order the speaker saw them.
    public func pairs(includeTargeted: Bool) -> [CalibrationPair] {
        var sections: [Section] = [.words, .phrases]
        if includeTargeted { sections.append(.targeted) }
        return sections.flatMap { prompts($0) }.compactMap { prompt in
            takes[prompt.id].map { CalibrationPair(reference: prompt.text, heard: $0.cleaned, raw: $0.raw) }
        }
    }
}
