import SwiftUI
import ShepitCore

// MARK: Look

enum WidgetPalette {
    static let text = Color(red: 0.957, green: 0.949, blue: 0.933)
    static let dim = text.opacity(0.46)
    static let ok = Color(red: 0.525, green: 0.812, blue: 0.627)
    static let amber = Color(red: 0.918, green: 0.682, blue: 0.333)
    static let rec = Color(red: 1, green: 0.38, blue: 0.35)
    static let spring = Animation.spring(response: 0.4, dampingFraction: 0.72)
}

/// Black translucent glass: a blurred backdrop under a black tint, a hairline edge and a soft shadow.
struct Glass<S: InsettableShape>: ViewModifier {
    var shape: S
    var opacity: Double

    func body(content: Content) -> some View {
        content
            .background {
                shape.fill(Color.black.opacity(opacity))
                    .background(.ultraThinMaterial, in: shape)
                    .shadow(color: .black.opacity(0.38), radius: 10, y: 4)
            }
            .overlay { shape.strokeBorder(Color.white.opacity(0.15), lineWidth: 0.75) }
    }
}

extension View {
    func glass<S: InsettableShape>(_ shape: S, opacity: Double) -> some View {
        modifier(Glass(shape: shape, opacity: opacity))
    }
}

struct KeyCap: View {
    let label: String

    var body: some View {
        Text(label)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(WidgetPalette.text)
            .padding(.horizontal, 5)
            .padding(.vertical, 1.5)
            .background(RoundedRectangle(cornerRadius: 4).fill(Color.white.opacity(0.1)))
            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(Color.white.opacity(0.12), lineWidth: 0.75))
    }
}

/// One line of words that keeps the newest ones in view; tentative words are dimmer.
struct LiveLine: View {
    var stable: String
    var unstable: String = ""
    var dim = false
    var color = WidgetPalette.text
    var size: CGFloat = 13

    var body: some View {
        let lead = Text(stable).foregroundStyle(dim ? WidgetPalette.dim : color)
        let tail = Text(unstable.isEmpty ? "" : (stable.isEmpty ? "" : " ") + unstable).foregroundStyle(WidgetPalette.dim)
        (lead + tail)
            .font(.system(size: size))
            .lineLimit(1)
            .truncationMode(.head)
    }
}

/// What the caption shows for the current phase: label, words, and whether they are dimmed.
struct CaptionContent {
    var label: String?
    var stable: String
    var unstable: String = ""
    var dim = false
    var color = WidgetPalette.text

    @MainActor static func of(_ m: WidgetModel) -> CaptionContent? {
        switch m.phase {
        case .idle: return nil
        case .listening: return m.live.isEmpty ? nil : CaptionContent(stable: m.live.stable, unstable: m.live.unstable)
        case .processing(let stage): return CaptionContent(label: stage.label, stable: m.live.full, dim: true)
        case .done(let text): return CaptionContent(stable: text)
        case .failed(let message): return CaptionContent(stable: message, color: WidgetPalette.amber)
        }
    }
}

/// Bars that follow the voice level. Mirrored around the middle for the pill, newest on the right otherwise.
/// While processing they turn into a slow travelling wave.
struct LevelBars: View {
    @ObservedObject var model: WidgetModel
    var count: Int
    var mirrored: Bool
    var maxHeight: CGFloat
    var width: CGFloat = 3
    var spacing: CGFloat = 3

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: !model.isProcessing)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            HStack(spacing: spacing) {
                ForEach(0..<count, id: \.self) { i in
                    Capsule()
                        .fill(WidgetPalette.text.opacity(opacity(i, t)))
                        .frame(width: width, height: height(i, t))
                }
            }
        }
        .frame(height: maxHeight)
        .animation(.linear(duration: 0.08), value: model.levels)
    }

    private func height(_ i: Int, _ t: TimeInterval) -> CGFloat {
        let d = abs(Double(i) - Double(count - 1) / 2)
        if model.isProcessing {
            return 3 + 3.5 * CGFloat(0.5 + 0.5 * sin(t * 7 - Double(i) * 0.7))
        }
        guard model.isListening else { return 3 }
        let ago = mirrored ? Int((d * 2).rounded()) : (count - 1 - i) * 2
        let shrink = mirrored ? 1 - d * 0.06 : 1 - abs(Double(i) - Double(count - 1) / 2) * 0.1
        return 3 + model.level(ago: ago) * (maxHeight - 3) * CGFloat(shrink)
    }

    private func opacity(_ i: Int, _ t: TimeInterval) -> Double {
        guard model.isProcessing else { return 1 }
        return 0.3 + 0.7 * (0.5 + 0.5 * sin(t * 7 - Double(i) * 0.7))
    }
}

/// Three steps (transcribe, format, insert) with the current one filling.
struct StepBar: View {
    var step: Int
    var labels: [String]? = nil
    var height: CGFloat = 2

    var body: some View {
        HStack(spacing: 6) {
            ForEach(0..<3, id: \.self) { i in
                StepSegment(state: i < step ? .done : i == step ? .active : .waiting, label: labels?[i], height: height)
                    .id(i == step ? "active-\(step)" : "seg-\(i)")
            }
        }
    }
}

private struct StepSegment: View {
    enum State { case waiting, active, done }
    var state: State
    var label: String?
    var height: CGFloat
    @SwiftUI.State private var fill: CGFloat = 0

    var body: some View {
        ZStack(alignment: .leading) {
            Capsule().fill(Color.white.opacity(label == nil ? 0.12 : 0.06))
            GeometryReader { geo in
                Capsule()
                    .fill(Color.white.opacity(label == nil ? 0.9 : 0.14))
                    .frame(width: geo.size.width * (state == .done ? 1 : state == .active ? fill : 0))
            }
            if let label {
                Text(label)
                    .font(.system(size: 11))
                    .foregroundStyle(state == .waiting ? WidgetPalette.dim : WidgetPalette.text)
                    .frame(maxWidth: .infinity)
            }
        }
        .frame(height: height)
        .clipShape(Capsule())
        .onAppear {
            guard state == .active else { return }
            withAnimation(.easeOut(duration: 1.4)) { fill = 0.92 }
        }
    }
}

// MARK: Styles

/// 1. A dash at the bottom that springs into a pill with a mirrored waveform; words float in a caption above.
struct FlowPillView: View {
    @ObservedObject var model: WidgetModel

    var body: some View {
        let pill = WidgetLayout.flowPill(model)
        VStack(spacing: 8) {
            if WidgetLayout.flowPillShowsCaption(model), let caption = CaptionContent.of(model) {
                // As wide as the words up to the limit; past it the line keeps its newest words.
                ViewThatFits(in: .horizontal) {
                    captionBody(caption).fixedSize()
                    captionBody(caption).frame(width: WidgetLayout.captionMaxWidth)
                }
                .glass(RoundedRectangle(cornerRadius: 12, style: .continuous), opacity: model.glassOpacity)
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            ZStack {
                if model.showsHover {
                    HStack(spacing: 4) {
                        Text("Hold")
                        KeyCap(label: model.keyLabel)
                        Text("to talk · click for hands-free")
                    }
                    .font(.system(size: 12))
                    .foregroundStyle(WidgetPalette.text.opacity(0.82))
                    .fixedSize()
                    .transition(.opacity)
                }
                if model.isListening || model.isProcessing {
                    LevelBars(model: model, count: 13, mirrored: true, maxHeight: 24)
                        .offset(x: model.isHandsFree ? -11 : 0)
                        .transition(.opacity)
                }
                if model.isHandsFree {
                    RoundedRectangle(cornerRadius: 3).fill(WidgetPalette.text.opacity(0.9))
                        .frame(width: 10, height: 10)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .padding(.trailing, 11)
                        .transition(.opacity)
                }
                if case .done = model.phase {
                    Image(systemName: "checkmark").font(.system(size: 13, weight: .bold)).foregroundStyle(WidgetPalette.ok)
                        .transition(.scale(scale: 0.5).combined(with: .opacity))
                }
                if case .failed = model.phase {
                    Image(systemName: "exclamationmark").font(.system(size: 14, weight: .bold)).foregroundStyle(WidgetPalette.amber)
                        .transition(.scale(scale: 0.5).combined(with: .opacity))
                }
            }
            .frame(width: pill.width, height: pill.height)
            .clipShape(Capsule())
            .glass(Capsule(), opacity: model.glassOpacity)
        }
        .animation(WidgetPalette.spring, value: model.phase)
        .animation(WidgetPalette.spring, value: model.hovering)
        .animation(WidgetPalette.spring, value: model.live.isEmpty)
    }

    private func captionBody(_ caption: CaptionContent) -> some View {
        HStack(spacing: 6) {
            if let label = caption.label {
                Text(label).font(.system(size: 11.5)).foregroundStyle(WidgetPalette.dim).fixedSize()
            }
            LiveLine(stable: caption.stable, unstable: caption.unstable, dim: caption.dim, color: caption.color)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
    }
}

/// 2. A wide bar built around the words, like subtitles, with the target app on the right.
struct CaptionBarView: View {
    @ObservedObject var model: WidgetModel

    var body: some View {
        let size = WidgetLayout.captionBar(model)
        ZStack {
            if model.isIdle {
                HStack(spacing: 8) {
                    Text("Shepit")
                    KeyCap(label: model.keyLabel)
                    if model.hovering { Text("hold to talk · click for hands-free").transition(.opacity) }
                }
                .font(.system(size: 12.5))
                .foregroundStyle(WidgetPalette.text.opacity(0.78))
                .lineLimit(1)
                .fixedSize()
                .transition(.opacity)
            } else {
                HStack(spacing: 10) {
                    ZStack {
                        Circle().fill(Color.white.opacity(0.07))
                        switch model.phase {
                        case .done: Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(WidgetPalette.ok)
                        case .failed: Image(systemName: "exclamationmark").font(.system(size: 13, weight: .bold)).foregroundStyle(WidgetPalette.amber)
                        default: LevelBars(model: model, count: 5, mirrored: false, maxHeight: 18, width: 2.5, spacing: 2)
                        }
                    }
                    .frame(width: 28, height: 28)

                    if let caption = CaptionContent.of(model) {
                        LiveLine(stable: caption.stable, unstable: caption.unstable, dim: caption.dim,
                                 color: caption.color, size: 14)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    } else {
                        Spacer(minLength: 0)
                    }

                    HStack(spacing: 6) {
                        switch model.phase {
                        case .listening: Text(model.elapsedText).font(.system(size: 12).monospacedDigit())
                        case .processing(let stage): Text(stage.label.lowercased()).font(.system(size: 12))
                        case .done: Text("inserted").font(.system(size: 12))
                        default: EmptyView()
                        }
                        if let target = model.target, !isFailed {
                            Text(target)
                                .font(.system(size: 11))
                                .lineLimit(1)
                                .padding(.horizontal, 7).padding(.vertical, 2)
                                .background(Capsule().fill(Color.white.opacity(0.08)))
                        }
                        if model.isHandsFree {
                            ZStack {
                                Circle().fill(Color.white.opacity(0.1))
                                RoundedRectangle(cornerRadius: 2).fill(WidgetPalette.text).frame(width: 8, height: 8)
                            }
                            .frame(width: 22, height: 22)
                        }
                    }
                    .foregroundStyle(WidgetPalette.text.opacity(0.6))
                    .fixedSize()
                }
                .padding(.leading, 7)
                .padding(.trailing, 8)
                .transition(.opacity)
            }
        }
        .frame(width: size.width, height: size.height)
        .overlay(alignment: .bottom) {
            if let stage = model.stage {
                StepBar(step: stage.step).padding(.horizontal, 18).transition(.opacity)
            }
        }
        .clipShape(Capsule())
        .glass(Capsule(), opacity: model.glassOpacity)
        .animation(WidgetPalette.spring, value: model.phase)
        .animation(WidgetPalette.spring, value: model.hovering)
    }

    private var isFailed: Bool {
        if case .failed = model.phase { return true }
        return false
    }
}

/// 4. One shape that morphs from a compact status line into a card with two lines of text and the steps.
struct IslandView: View {
    @ObservedObject var model: WidgetModel

    var body: some View {
        let layout = WidgetLayout.island(model)
        let shape = RoundedRectangle(cornerRadius: layout.radius, style: .continuous)
        VStack(alignment: .leading, spacing: 0) {
            header
                .frame(height: min(36, layout.size.height))
                .padding(.horizontal, 14)
            if showsBody {
                bodyText
                    .padding(.horizontal, 16)
                    .transition(.opacity)
            }
            Spacer(minLength: 0)
            if let stage = model.stage {
                StepBar(step: stage.step, labels: ["Transcribe", "Format", "Insert"], height: 22)
                    .padding(.horizontal, 14)
                    .padding(.bottom, 12)
                    .transition(.opacity)
            }
        }
        .frame(width: layout.size.width, height: layout.size.height, alignment: .top)
        .clipShape(shape)
        .glass(shape, opacity: model.glassOpacity)
        .animation(WidgetPalette.spring, value: model.phase)
        .animation(WidgetPalette.spring, value: model.hovering)
        .animation(WidgetPalette.spring, value: model.live.isEmpty)
    }

    private var showsBody: Bool {
        switch model.phase {
        case .listening: return !model.live.isEmpty
        case .processing, .done: return true
        default: return false
        }
    }

    @ViewBuilder private var header: some View {
        HStack(spacing: 8) {
            switch model.phase {
            case .idle:
                Spacer(minLength: 0)
                if model.hovering {
                    Text("Hold").foregroundStyle(WidgetPalette.text.opacity(0.8))
                    KeyCap(label: model.keyLabel)
                    Text("· click for hands-free").foregroundStyle(WidgetPalette.text.opacity(0.8))
                } else {
                    Image(systemName: "mic.fill").font(.system(size: 10)).foregroundStyle(WidgetPalette.text.opacity(0.8))
                    KeyCap(label: model.keyLabel)
                }
                Spacer(minLength: 0)
            case .listening(let handsFree):
                Circle().fill(WidgetPalette.rec).frame(width: 7, height: 7)
                Text(handsFree ? "Hands-free" : "Listening").foregroundStyle(WidgetPalette.text.opacity(0.8))
                Spacer(minLength: 0)
                if handsFree { Image(systemName: "lock.fill").font(.system(size: 9)).foregroundStyle(WidgetPalette.text.opacity(0.8)) }
                LevelBars(model: model, count: 9, mirrored: false, maxHeight: 16, width: 2.5, spacing: 2)
                Text(model.elapsedText).font(.system(size: 12).monospacedDigit()).foregroundStyle(WidgetPalette.text.opacity(0.55))
            case .processing(let stage):
                Text(stage.label).foregroundStyle(WidgetPalette.text.opacity(0.8))
                Spacer(minLength: 0)
            case .done:
                Image(systemName: "checkmark").font(.system(size: 11, weight: .bold)).foregroundStyle(WidgetPalette.ok)
                Text(model.target.map { "Inserted into \($0)" } ?? "Inserted").foregroundStyle(WidgetPalette.text.opacity(0.8))
                Spacer(minLength: 0)
            case .failed(let message):
                Image(systemName: "exclamationmark").font(.system(size: 12, weight: .bold)).foregroundStyle(WidgetPalette.amber)
                Text(message).foregroundStyle(WidgetPalette.amber).lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .font(.system(size: 12.5))
        .lineLimit(1)
    }

    @ViewBuilder private var bodyText: some View {
        switch model.phase {
        case .done(let text):
            Text(text).font(.system(size: 14.5)).foregroundStyle(WidgetPalette.text).lineLimit(2)
        case .processing:
            Text(model.live.full).font(.system(size: 14.5)).foregroundStyle(WidgetPalette.dim).lineLimit(2).truncationMode(.head)
        default:
            (Text(model.live.stable).foregroundStyle(WidgetPalette.text)
                + Text((model.live.stable.isEmpty ? "" : " ") + model.live.unstable).foregroundStyle(WidgetPalette.dim))
                .font(.system(size: 14.5))
                .lineLimit(2)
                .truncationMode(.head)
        }
    }
}

/// The selected style, placed at the bottom middle of whatever space it gets.
struct WidgetStyleView: View {
    @ObservedObject var model: WidgetModel

    var body: some View {
        Group {
            switch model.style {
            case .flowPill: FlowPillView(model: model)
            case .captionBar: CaptionBarView(model: model)
            case .island: IslandView(model: model)
            }
        }
        .environment(\.colorScheme, .dark)
    }
}

/// Root of the floating panel: the widget pinned to the bottom middle with room for its shadow.
struct WidgetRootView: View {
    @ObservedObject var model: WidgetModel

    var body: some View {
        WidgetStyleView(model: model)
            .padding(.horizontal, WidgetLayout.margin.width)
            .padding(.vertical, WidgetLayout.margin.height)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
    }
}
