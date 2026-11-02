import AppKit
import SwiftUI
import ShepitCore

/// What each release adds that a user can see. Bug-fix releases get no entry, so they stay quiet.
enum WhatsNew {
    struct Item {
        let symbol: String
        let title: String
        let detail: String
    }

    /// Newest first.
    static let releases: [(version: String, items: [Item])] = [
        ("0.6.0", [
            Item(symbol: "stethoscope", title: "Doctor",
                 detail: "Finds what keeps Shepit from working, like a permission macOS still holds for an older build, and fixes it in a click. Open it from the menu bar icon or the widget menu."),
            Item(symbol: "globe", title: "Ukrainian and English only",
                 detail: "Russian or Polish words no longer slip into your text. Settings → General can narrow recognition to one language."),
        ]),
        ("0.5.0", [
            Item(symbol: "text.cursor", title: "Review before pasting",
                 detail: "Hold Shift as you release the dictation key to check and edit the text first. Enter pastes, Esc cancels. Settings → General turns it on for every dictation."),
        ]),
        ("0.4.1", [
            Item(symbol: "trash", title: "Clear history",
                 detail: "The History window has a Clear History button. It deletes every dictation and recording after you confirm."),
        ]),
        ("0.3.0", [
            Item(symbol: "arrow.down.circle", title: "Updates from GitHub",
                 detail: "Shepit checks once a day and installs a new version in one click. Settings → About has the switch and a Check now button."),
            Item(symbol: "lock.shield", title: "Permissions survive updates",
                 detail: "Releases are now signed with one certificate, so macOS keeps Accessibility and Microphone across updates. Grant them once more after this one."),
        ]),
        ("0.2.0", [
            Item(symbol: "waveform", title: "Words while you speak",
                 detail: "The widget shows what it hears as you talk. Dim words may still change."),
            Item(symbol: "timer", title: "Long dictations finish sooner",
                 detail: "Whisper works through long dictations while you speak. A 46-second prompt is ready about 4 seconds after you let go, down from 10."),
            Item(symbol: "rectangle.3.group", title: "Three widget styles",
                 detail: "Flow pill, caption bar or island, in Settings → Appearance."),
        ]),
    ]

    static var current: String { AppVersion.current }

    /// The newest release with an entry that is newer than `seen` and not newer than the running app.
    static func unseen(after seen: String) -> (version: String, items: [Item])? {
        releases
            .filter { isNewer($0.version, than: seen) && !isNewer($0.version, than: current) }
            .max { isNewer($1.version, than: $0.version) }
    }

    static func isNewer(_ a: String, than b: String) -> Bool { AppVersion.isNewer(a, than: b) }
}

struct WhatsNewCard: View {
    let heading: String
    /// Shown as a chip next to the heading; nil for cards that are not about a release.
    let version: String?
    let items: [WhatsNew.Item]
    let secondaryTitle: String
    var secondary: () -> Void
    var close: () -> Void
    @State private var shown = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                ShepitMark().frame(width: 18, height: 18)
                Text(heading).font(.system(size: 15, weight: .semibold))
                if let version {
                    Text(version)
                        .font(.system(size: 11, weight: .medium))
                        .padding(.horizontal, 7).padding(.vertical, 1)
                        .background(Capsule().fill(Color.white.opacity(0.1)))
                        .foregroundStyle(WidgetPalette.text.opacity(0.75))
                }
            }
            VStack(alignment: .leading, spacing: 10) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    HStack(alignment: .top, spacing: 11) {
                        Image(systemName: item.symbol)
                            .font(.system(size: 13))
                            .frame(width: 30, height: 30)
                            .background(RoundedRectangle(cornerRadius: 9).fill(Color.white.opacity(0.08)))
                        VStack(alignment: .leading, spacing: 1) {
                            Text(item.title).font(.system(size: 13.5, weight: .medium))
                            Text(item.detail).font(.system(size: 12.5)).foregroundStyle(WidgetPalette.text.opacity(0.62))
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .opacity(shown ? 1 : 0)
                    .offset(y: shown ? 0 : 6)
                    .animation(.easeOut(duration: 0.45).delay(0.15 + Double(index) * 0.12), value: shown)
                }
            }
            HStack {
                Spacer()
                Button(secondaryTitle, action: secondary).buttonStyle(QuietButtonStyle())
                Button("Got it", action: close).buttonStyle(PrimaryButtonStyle())
            }
        }
        .foregroundStyle(WidgetPalette.text)
        .padding(16)
        .frame(width: 380)
        .glass(RoundedRectangle(cornerRadius: 18, style: .continuous), opacity: 0.74)
        .scaleEffect(shown ? 1 : 0.94, anchor: .bottom)
        .offset(y: shown ? 0 : 18)
        .opacity(shown ? 1 : 0)
        .environment(\.colorScheme, .dark)
        .onAppear { withAnimation(WidgetPalette.spring) { shown = true } }
    }
}

/// Shows a card above the widget: the What's New card after an update, or a one-time tip.
@MainActor
final class WhatsNewController {
    private var panel: NSPanel?
    var onClose: () -> Void = {}

    func show(heading: String, version: String?, items: [WhatsNew.Item], secondaryTitle: String,
              above widget: NSRect, secondary: @escaping () -> Void) {
        dismiss(notify: false)
        let card = WhatsNewCard(heading: heading, version: version, items: items, secondaryTitle: secondaryTitle,
                                secondary: { [weak self] in
            self?.dismiss()
            secondary()
        }, close: { [weak self] in self?.dismiss() })
            .padding(24)
        let hosting = NSHostingView(rootView: card)
        let size = hosting.fittingSize
        let panel = NSPanel(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless, .nonactivatingPanel],
                            backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentView = hosting
        var origin = NSPoint(x: widget.midX - size.width / 2, y: widget.maxY - 18)
        if let visible = NSScreen.screens.first(where: { $0.frame.intersects(widget) })?.visibleFrame {
            origin.x = min(max(origin.x, visible.minX), visible.maxX - size.width)
            origin.y = min(origin.y, visible.maxY - size.height)
        }
        panel.setFrameOrigin(origin)
        panel.orderFrontRegardless()
        self.panel = panel
    }

    var isShown: Bool { panel != nil }

    func dismiss(notify: Bool = true) {
        guard let panel else { return }
        panel.orderOut(nil)
        self.panel = nil
        if notify { onClose() }
    }
}
