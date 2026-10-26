import AppKit
import Combine
import SwiftUI
import ShepitCore

/// Offers a new release in the top right corner, then shows the download and install in place.
struct UpdateCard: View {
    @ObservedObject var updates: UpdateChecker
    var close: () -> Void
    @State private var shown = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                ShepitMark().frame(width: 18, height: 18)
                Text(title).font(.system(size: 15, weight: .semibold))
            }
            content
        }
        .foregroundStyle(WidgetPalette.text)
        .padding(16)
        .frame(width: 380, alignment: .leading)
        .glass(RoundedRectangle(cornerRadius: 18, style: .continuous), opacity: 0.74)
        .scaleEffect(shown ? 1 : 0.94, anchor: .top)
        .offset(y: shown ? 0 : -14)
        .opacity(shown ? 1 : 0)
        .environment(\.colorScheme, .dark)
        .animation(.easeOut(duration: 0.25), value: updates.state)
        .onAppear { withAnimation(WidgetPalette.spring) { shown = true } }
    }

    private var title: String {
        switch updates.state {
        case .idle, .checking: return "Checking for updates"
        case .upToDate: return "Shepit is up to date"
        case .available(let release): return "Shepit \(release.version) is out"
        case .downloading(let release, _), .installing(let release): return "Updating to \(release.version)"
        case .failed(_, let release): return release == nil ? "Could not check for updates" : "The update did not install"
        }
    }

    @ViewBuilder private var content: some View {
        switch updates.state {
        case .idle, .checking:
            ProgressView().progressViewStyle(.linear).tint(WidgetPalette.text)
            buttons { Button("Close", action: close).buttonStyle(QuietButtonStyle()) }

        case .upToDate:
            detail("You have the latest version, \(AppVersion.current).")
            buttons { Button("OK", action: close).buttonStyle(PrimaryButtonStyle()) }

        case .available(let release):
            detail("You have \(AppVersion.current). Shepit downloads the new version from GitHub, checks that it is signed like this one, puts it in place and opens again.")
            Button("What changed") { NSWorkspace.shared.open(release.pageURL) }
                .buttonStyle(LinkButtonStyle())
            buttons {
                Button("Skip this version") {
                    updates.skip(release)
                    close()
                }
                .buttonStyle(QuietButtonStyle())
                Spacer()
                Button("Later", action: close).buttonStyle(QuietButtonStyle())
                Button("Install and reopen") { Task { await updates.install(release) } }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }

        case .downloading(let release, let fraction):
            ProgressView(value: fraction).progressViewStyle(.linear).tint(WidgetPalette.text)
            detail(downloadText(release, fraction))

        case .installing:
            ProgressView().progressViewStyle(.linear).tint(WidgetPalette.text)
            detail("Checking the signature and putting the new version in place. Shepit reopens on its own.")

        case .failed(let message, let release):
            detail(message)
            buttons {
                Spacer()
                Button("Close", action: close).buttonStyle(QuietButtonStyle())
                Button("Open release page") {
                    NSWorkspace.shared.open(release?.pageURL ?? ReleaseFeed.releasesPage)
                    close()
                }
                .buttonStyle(PrimaryButtonStyle())
            }
        }
    }

    private func downloadText(_ release: AppRelease, _ fraction: Double) -> String {
        guard release.diskImageSize > 0 else { return "Downloading… \(Int(fraction * 100))%" }
        let total = Double(release.diskImageSize) / 1_000_000
        return String(format: "Downloading… %.0f of %.0f MB", total * fraction, total)
    }

    private func detail(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12.5))
            .foregroundStyle(WidgetPalette.text.opacity(0.65))
            .fixedSize(horizontal: false, vertical: true)
    }

    private func buttons<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        HStack(spacing: 4) {
            Spacer(minLength: 0)
            content()
        }
    }
}

/// An underlined text button for links inside a card.
struct LinkButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12.5))
            .underline()
            .foregroundStyle(WidgetPalette.text.opacity(configuration.isPressed ? 0.5 : 0.8))
    }
}

/// The update card's panel. One at a time; showing it again while it is up does nothing.
@MainActor
final class UpdatePanelController {
    private var panel: NSPanel?
    private var resize: AnyCancellable?

    func show(_ updates: UpdateChecker) {
        guard panel == nil else { return }
        let card = UpdateCard(updates: updates) { [weak self] in self?.dismiss() }.padding(20)
        let hosting = NSHostingView(rootView: card)
        hosting.sizingOptions = [.intrinsicContentSize]
        let size = hosting.fittingSize
        let panel = NSPanel(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless, .nonactivatingPanel],
                            backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentView = hosting
        // Under the menu bar on the right, where the status item and notifications are.
        if let visible = (NSScreen.main ?? NSScreen.screens.first)?.visibleFrame {
            panel.setFrameTopLeftPoint(NSPoint(x: visible.maxX - size.width, y: visible.maxY))
        }
        panel.orderFrontRegardless()
        self.panel = panel

        // The card changes height between offering, downloading and failing; keep its top edge in place.
        resize = updates.$state.receive(on: RunLoop.main).sink { [weak panel, weak hosting] _ in
            MainActor.assumeIsolated {
                guard let panel, let hosting else { return }
                DispatchQueue.main.async {
                    let size = hosting.fittingSize
                    let top = panel.frame.maxY
                    panel.setFrame(NSRect(x: panel.frame.maxX - size.width, y: top - size.height,
                                          width: size.width, height: size.height), display: true)
                }
            }
        }
    }

    func dismiss() {
        resize = nil
        panel?.orderOut(nil)
        panel = nil
    }
}
