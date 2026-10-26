import SwiftUI
import ShepitCore

/// Plays a scripted dictation on its own `WidgetModel`, for the style previews in Settings and the welcome tour.
@MainActor
final class WidgetDemo: ObservableObject {
    let model = WidgetModel()
    /// Called with the final text each time the scripted dictation is inserted.
    var onInserted: ((String) -> Void)?
    var onReset: (() -> Void)?

    private var task: Task<Void, Never>?
    private var levelTask: Task<Void, Never>?

    static let heard = ["Зроби", "рефакторинг", "хендл", "сабміт", "і", "додай", "тест", "для", "юзер", "сервіс"]
    static let inserted = "Зроби рефакторинг handleSubmit і додай тест для UserService.swift."

    init(style: WidgetStyle) {
        model.style = style
        model.target = "Terminal"
    }

    /// Loops the dictation until `stop()`; `once` plays it a single time, starting from listening.
    func start(once: Bool = false) {
        stop()
        task = Task { [weak self] in
            repeat {
                guard let self else { return }
                await self.play(withIdle: !once)
            } while !once && !Task.isCancelled
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        levelTask?.cancel()
        levelTask = nil
    }

    private func play(withIdle: Bool) async {
        let m = model
        if withIdle {
            m.phase = .idle
            m.live = .empty
            onReset?()
            guard await pause(1.6) else { return }
            m.hovering = true
            guard await pause(1.3) else { return }
            m.hovering = false
        }
        m.phase = .listening(handsFree: false)
        m.elapsed = 0
        startLevels()
        let words = Self.heard
        for i in 1...words.count {
            let split = max(0, i - 2)
            m.live = LivePreview.Text(stable: words[0..<split].joined(separator: " "),
                                      unstable: words[split..<i].joined(separator: " "))
            guard await pause(0.36) else { return }
        }
        guard await pause(0.5) else { return }
        levelTask?.cancel()
        for (stage, seconds) in [(WidgetModel.Stage.transcribing, 0.75), (.formatting, 1.15), (.inserting, 0.4)] {
            m.phase = .processing(stage)
            guard await pause(seconds) else { return }
        }
        m.phase = .done(Self.inserted)
        m.live = .empty
        onInserted?(Self.inserted)
        _ = await pause(2.6)
    }

    /// A voice-like level: syllables at about 4 Hz with a breath every few seconds.
    private func startLevels() {
        levelTask?.cancel()
        let m = model
        levelTask = Task { [weak self] in
            let start = Date()
            while !Task.isCancelled, self != nil {
                let t = Date().timeIntervalSince(start)
                let phrase = t.truncatingRemainder(dividingBy: 2.7) > 2.3 ? 0.1 : 1
                let syllable = pow(0.5 + 0.5 * sin(t * 26 + sin(t * 3.1) * 2.2), 1.5)
                let level = min(1, phrase * (0.16 + 0.84 * syllable * (0.6 + 0.4 * sin(t * 1.3 + 1))) + Double.random(in: 0...0.07))
                m.pushLevel(Float(level))
                m.elapsed = t
                try? await Task.sleep(for: .milliseconds(40))
            }
        }
    }

    private func pause(_ seconds: Double) async -> Bool {
        try? await Task.sleep(for: .seconds(seconds))
        return !Task.isCancelled
    }
}

/// A slice of desktop wallpaper with a widget at the bottom, scaled down so the style fits at its widest
/// and tallest, never above `maxScale`.
struct DemoStage: View {
    @ObservedObject var model: WidgetModel
    var maxScale: CGFloat = 0.9
    var height: CGFloat

    var body: some View {
        GeometryReader { geo in
            let widest = WidgetLayout.largest(model.style)
            let scale = min(maxScale, (geo.size.width - 24) / widest.width, (height - 28) / widest.height)
            WidgetStyleView(model: model)
                .frame(width: geo.size.width / scale, height: height / scale, alignment: .bottom)
                .padding(.bottom, 14 / scale)
                .scaleEffect(scale, anchor: .bottomLeading)
                .frame(width: geo.size.width, height: height, alignment: .bottomLeading)
                .animation(WidgetPalette.spring, value: scale)
        }
        .frame(height: height)
        .background(Wallpaper())
        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
    }
}

/// The muted teal and amber wallpaper used behind previews.
struct Wallpaper: View {
    var body: some View {
        ZStack {
            LinearGradient(colors: [Color(red: 0.18, green: 0.216, blue: 0.224), Color(red: 0.133, green: 0.157, blue: 0.165)],
                           startPoint: .topLeading, endPoint: .bottomTrailing)
            RadialGradient(colors: [Color(red: 0.353, green: 0.435, blue: 0.439), .clear], center: UnitPoint(x: 0.18, y: 0.08),
                           startRadius: 0, endRadius: 260)
            RadialGradient(colors: [Color(red: 0.478, green: 0.384, blue: 0.286), .clear], center: UnitPoint(x: 0.88, y: 0.96),
                           startRadius: 0, endRadius: 240)
        }
    }
}
