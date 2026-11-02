import AppKit
import Combine
import SwiftUI
import ShepitCore

/// Borderless panel that takes keyboard focus without activating Shepit, so the app being dictated into stays
/// frontmost and receives the paste once the panel is gone.
final class ReviewPanel: NSPanel {
    init(contentRect: NSRect) {
        super.init(contentRect: contentRect, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .statusBar
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false // the card draws its own shadow
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        becomesKeyOnlyIfNeeded = false
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    /// Shepit is not the active app while this non-activating panel is key, so the hidden main menu's Edit items
    /// do not apply here and Cmd-X/C/V/A/Z are handled below.
    /// On a non-Latin layout (Ukrainian) the key code stands in for the letter; Latin layouts such as Dvorak keep
    /// their own letters.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard flags.contains(.command), flags.isDisjoint(with: [.control, .option]) else {
            return super.performKeyEquivalent(with: event)
        }
        let typed = event.charactersIgnoringModifiers?.lowercased() ?? ""
        let qwerty: [UInt16: String] = [7: "x", 8: "c", 9: "v", 0: "a", 6: "z"]
        let char = typed.unicodeScalars.allSatisfy(\.isASCII) ? typed : (qwerty[event.keyCode] ?? typed)
        let shift = flags.contains(.shift)
        let action: Selector?
        switch char {
        case "x": action = shift ? nil : #selector(NSText.cut(_:))
        case "c": action = shift ? nil : #selector(NSText.copy(_:))
        case "v": action = shift ? nil : #selector(NSText.paste(_:))
        case "a": action = shift ? nil : #selector(NSText.selectAll(_:))
        case "z": action = shift ? Selector(("redo:")) : Selector(("undo:"))
        default: action = nil
        }
        if let action, NSApp.sendAction(action, to: nil, from: self) { return true }
        return super.performKeyEquivalent(with: event)
    }
}

/// Owns the text view for one review. The view is built here rather than in SwiftUI so the controller can focus it
/// right after the panel is shown, without waiting for SwiftUI to create it.
@MainActor
final class ReviewEditor: NSObject, ObservableObject, NSTextViewDelegate {
    static let minHeight: CGFloat = 60
    static let maxHeight: CGFloat = 200

    let scrollView = NSScrollView()
    let textView = NSTextView()
    /// Height the text needs, clamped to the card's range.
    @Published private(set) var contentHeight: CGFloat = ReviewEditor.minHeight
    /// App name shown on the chip. Follows the frontmost app while the panel is open.
    @Published var target: String?
    var onConfirm: @MainActor (String) -> Void = { _ in }
    var onCancel: @MainActor () -> Void = {}

    init(text: String, target: String? = nil) {
        self.target = target
        super.init()
        textView.delegate = self
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.drawsBackground = false
        textView.font = .systemFont(ofSize: 14)
        textView.textColor = NSColor(WidgetPalette.text)
        textView.insertionPointColor = .white
        // The text is pasted as typed: nothing may rewrite it behind the user's back.
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.isAutomaticTextCompletionEnabled = false
        textView.isContinuousSpellCheckingEnabled = false
        textView.isGrammarCheckingEnabled = false
        textView.textContainerInset = .zero
        textView.textContainer?.lineFragmentPadding = 0
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.string = text
        textView.typingAttributes = [.font: NSFont.systemFont(ofSize: 14), .foregroundColor: NSColor(WidgetPalette.text)]
        scrollView.documentView = textView
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay
        scrollView.borderType = .noBorder
        measure()
    }

    var text: String { textView.string }

    func focus() {
        textView.window?.makeFirstResponder(textView)
        textView.setSelectedRange(NSRange(location: (textView.string as NSString).length, length: 0))
    }

    func measure() {
        guard let container = textView.textContainer, let layout = textView.layoutManager else { return }
        // Before the view has a width the container is unbounded and the text would measure as one line.
        if scrollView.frame.width > 0 { container.containerSize.width = scrollView.contentSize.width }
        layout.ensureLayout(for: container)
        let needed = ceil(layout.usedRect(for: container).height) + 4
        let clamped = min(max(needed, Self.minHeight), Self.maxHeight)
        if clamped != contentHeight { contentHeight = clamped }
    }

    func textDidChange(_ notification: Notification) { measure() }

    func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(NSResponder.insertNewline(_:)):
            // Return that commits input-method text is not a confirm.
            if textView.hasMarkedText() { return false }
            let flags = NSApp.currentEvent?.modifierFlags ?? []
            if flags.contains(.shift) || flags.contains(.option) {
                textView.insertNewlineIgnoringFieldEditor(nil)
                return true
            }
            onConfirm(textView.string)
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            onCancel()
            return true
        default:
            return false
        }
    }
}

struct ReviewTextView: NSViewRepresentable {
    let editor: ReviewEditor

    func makeNSView(context: Context) -> NSScrollView { editor.scrollView }
    func updateNSView(_ nsView: NSScrollView, context: Context) {}
}

struct ReviewCard: View {
    @ObservedObject var editor: ReviewEditor

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Text("Review before pasting").font(.system(size: 15, weight: .semibold))
                if let target = editor.target {
                    Text(target)
                        .font(.system(size: 11, weight: .medium))
                        .padding(.horizontal, 7).padding(.vertical, 1)
                        .background(Capsule().fill(Color.white.opacity(0.1)))
                        .foregroundStyle(WidgetPalette.text.opacity(0.75))
                }
            }
            ReviewTextView(editor: editor)
                .frame(width: 412, height: editor.contentHeight)
            Text("Enter pastes · Shift+Enter new line · Esc cancels")
                .font(.system(size: 11.5))
                .foregroundStyle(WidgetPalette.text.opacity(0.5))
            HStack {
                Spacer()
                Button("Cancel") { editor.onCancel() }.buttonStyle(QuietButtonStyle())
                Button("Paste") { editor.onConfirm(editor.text) }.buttonStyle(PrimaryButtonStyle())
            }
        }
        .foregroundStyle(WidgetPalette.text)
        .padding(16)
        .frame(width: 440)
        .glass(RoundedRectangle(cornerRadius: 18, style: .continuous), opacity: 0.74)
        .environment(\.colorScheme, .dark)
        .padding(24)
    }
}

/// Shows the review card above the widget while the session is in `.reviewing`, and closes it in any other state.
@MainActor
final class ReviewPanelController {
    /// Time for the previous app to become key again after the panel closes, before Cmd-V goes out. A guess to be
    /// confirmed on a device.
    private static let focusReturnDelay: Duration = .milliseconds(80)

    private let session: DictationSession
    private let widget: WidgetController
    private var panel: ReviewPanel?
    private var editor: ReviewEditor?
    private var heightObserver: AnyCancellable?
    private var confirming = false
    private var activationObserver: NSObjectProtocol?
    private var cancellables = Set<AnyCancellable>()

    init(session: DictationSession, widget: WidgetController) {
        self.session = session
        self.widget = widget
        session.$state.receive(on: RunLoop.main).sink { [weak self] state in
            MainActor.assumeIsolated {
                if case .reviewing(let text) = state { self?.show(text: text) } else { self?.close() }
            }
        }.store(in: &cancellables)
    }

    private func show(text: String) {
        close()
        let editor = ReviewEditor(text: text, target: session.targetAppName)
        editor.onConfirm = { [weak self] text in self?.confirm(text) }
        editor.onCancel = { [weak self] in self?.cancel() }
        let hosting = NSHostingView(rootView: ReviewCard(editor: editor))
        let size = hosting.fittingSize
        let panel = ReviewPanel(contentRect: NSRect(origin: .zero, size: size))
        panel.contentView = hosting
        panel.setFrame(frame(for: size), display: false)
        panel.orderFrontRegardless()
        panel.makeKey()
        self.panel = panel
        self.editor = editor
        // Focus can drift while the card is open; the chip shows where the paste would land now.
        activationObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak editor] note in
            let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            guard let app, app.bundleIdentifier != Bundle.main.bundleIdentifier, let name = app.localizedName else { return }
            MainActor.assumeIsolated { editor?.target = name }
        }
        // The text area grows with the text; the panel follows, keeping its bottom edge above the widget.
        heightObserver = editor.$contentHeight.dropFirst().sink { [weak self, weak hosting] _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self, let hosting, let panel = self.panel else { return }
                    let size = hosting.fittingSize
                    var frame = panel.frame
                    frame.size = size
                    panel.setFrame(frame, display: true)
                }
            }
        }
        // The scroll view has its width only after the first layout; measure again so a long text starts tall.
        hosting.layoutSubtreeIfNeeded()
        editor.measure()
        editor.focus()
    }

    /// Above the resting widget frame, so the card does not move when the widget shrinks after a dictation.
    private func frame(for size: CGSize) -> NSRect {
        let widgetFrame = widget.restingFrame
        var origin = NSPoint(x: widgetFrame.midX - size.width / 2, y: widgetFrame.maxY - 18)
        if let visible = NSScreen.screens.first(where: { $0.frame.intersects(widgetFrame) })?.visibleFrame {
            origin.x = min(max(origin.x, visible.minX), visible.maxX - size.width)
            origin.y = min(origin.y, visible.maxY - size.height)
        }
        return NSRect(origin: origin, size: size)
    }

    private func close() {
        heightObserver = nil
        if let activationObserver { NSWorkspace.shared.notificationCenter.removeObserver(activationObserver) }
        activationObserver = nil
        panel?.orderOut(nil)
        panel = nil
        editor = nil
        confirming = false
    }

    private func confirm(_ text: String) {
        guard !confirming else { return }
        confirming = true
        // Cmd-V goes to whatever is key, so the panel must be gone before the session posts it.
        panel?.orderOut(nil)
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: Self.focusReturnDelay)
            self?.session.confirmReview(text)
        }
    }

    /// Esc also reaches the session through the hotkey monitor; discarding twice is harmless because the session
    /// consumes the pending review once.
    private func cancel() {
        session.discardReview()
    }
}
