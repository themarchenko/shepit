import SwiftUI
import ShepitCore

/// Everything the widget shows. `WidgetController` feeds it from the dictation session; `WidgetDemo` plays a
/// scripted dictation on it for the previews in Settings and the welcome tour.
@MainActor
final class WidgetModel: ObservableObject {
    enum Phase: Equatable {
        case idle
        case listening(handsFree: Bool)
        case processing(Stage)
        case done(String)
        case failed(String)
    }

    enum Stage: Int, Equatable {
        case loading, transcribing, formatting, inserting

        var label: String {
            switch self {
            case .loading: return "Loading model"
            case .transcribing: return "Transcribing"
            case .formatting: return "Formatting"
            case .inserting: return "Inserting"
            }
        }

        /// Position among the three steps the user sees; loading the model counts as transcribing.
        var step: Int { max(0, rawValue - 1) }
    }

    @Published var phase: Phase = .idle
    @Published var hovering = false
    @Published var levels: [Float] = Array(repeating: 0, count: 32)
    @Published var elapsed: TimeInterval = 0
    @Published var live: LivePreview.Text = .empty
    /// The app the text goes to, for the caption bar's chip.
    @Published var target: String?
    @Published var style: WidgetStyle = .flowPill
    @Published var glassOpacity: Double = 0.7
    @Published var keyLabel = "Right ⌥"

    var isListening: Bool {
        if case .listening = phase { return true }
        return false
    }

    var isHandsFree: Bool { phase == .listening(handsFree: true) }
    var isIdle: Bool { phase == .idle }
    var showsHover: Bool { phase == .idle && hovering }

    var isProcessing: Bool {
        if case .processing = phase { return true }
        return false
    }

    var stage: Stage? {
        if case .processing(let s) = phase { return s }
        return nil
    }

    var elapsedText: String {
        let s = Int(elapsed)
        return String(format: "%d:%02d", s / 60, s % 60)
    }

    func pushLevel(_ level: Float) {
        levels.removeFirst()
        levels.append(level)
    }

    /// The level `steps` samples back, newest first.
    func level(ago steps: Int) -> CGFloat {
        CGFloat(levels[max(0, levels.count - 1 - steps)])
    }
}

/// Shape sizes per style and phase. The panel that hosts the widget is sized from the same numbers so the
/// shape always has room to spring into its next size.
@MainActor
enum WidgetLayout {
    /// Space around the shape for its shadow. The panel clips anything outside it, so this must cover the
    /// shadow's blur plus its offset (`Glass`: radius 10, 4 down), or the shadow ends in a visible rectangle.
    static let margin = CGSize(width: 28, height: 26)
    static let captionMaxWidth: CGFloat = 440

    static func flowPill(_ m: WidgetModel) -> CGSize {
        switch m.phase {
        case .idle: return m.hovering ? CGSize(width: 262, height: 28) : CGSize(width: 40, height: 8)
        case .listening(let handsFree): return CGSize(width: handsFree ? 166 : 132, height: 32)
        case .processing: return CGSize(width: 112, height: 32)
        case .done, .failed: return CGSize(width: 44, height: 32)
        }
    }

    static func flowPillShowsCaption(_ m: WidgetModel) -> Bool {
        switch m.phase {
        case .idle: return false
        case .listening: return !m.live.isEmpty
        case .processing, .done, .failed: return true
        }
    }

    static func captionBar(_ m: WidgetModel) -> CGSize {
        switch m.phase {
        case .idle: return CGSize(width: m.hovering ? 330 : 178, height: 30)
        case .failed: return CGSize(width: 270, height: 42)
        default: return CGSize(width: 560, height: 42)
        }
    }

    static func island(_ m: WidgetModel) -> (size: CGSize, radius: CGFloat) {
        switch m.phase {
        case .idle: return m.hovering ? (CGSize(width: 252, height: 30), 15) : (CGSize(width: 96, height: 26), 13)
        case .listening: return m.live.isEmpty ? (CGSize(width: 270, height: 36), 18) : (CGSize(width: 410, height: 94), 24)
        case .processing: return (CGSize(width: 410, height: 122), 26)
        case .done: return (CGSize(width: 410, height: 94), 24)
        case .failed: return (CGSize(width: 254, height: 36), 18)
        }
    }

    /// The biggest the style's shape gets in any phase, for scaling previews so they never outgrow their stage.
    static func largest(_ style: WidgetStyle) -> CGSize {
        switch style {
        case .flowPill: return CGSize(width: captionMaxWidth + 24, height: 32 + 8 + 34)
        case .captionBar: return CGSize(width: 560, height: 42)
        case .island: return CGSize(width: 410, height: 122)
        }
    }

    /// The panel size that fits the current shape plus its shadow.
    static func envelope(_ m: WidgetModel) -> CGSize {
        var size: CGSize
        switch m.style {
        case .flowPill:
            size = flowPill(m)
            if flowPillShowsCaption(m) {
                size.width = max(size.width, captionMaxWidth + 24)
                size.height += 8 + 34
            }
        case .captionBar:
            size = captionBar(m)
        case .island:
            size = island(m).size
        }
        return CGSize(width: ceil(size.width + margin.width * 2), height: ceil(size.height + margin.height * 2))
    }
}
