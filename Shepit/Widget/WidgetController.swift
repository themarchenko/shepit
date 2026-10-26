import AppKit
import Combine
import SwiftUI
import ShepitCore

/// Owns the floating widget: position, idle fade, state display and the right-click menu.
///
/// The widget is placed by an anchor, the middle of its bottom edge, so it grows up and to both sides when
/// a dictation starts and shrinks back to the same spot. The anchor survives relaunches; when it is not on
/// any connected display (a monitor was unplugged) the widget goes to the bottom middle of the main one.
@MainActor
final class WidgetController: NSObject, NSMenuDelegate {
    var onOpenSettings: @MainActor () -> Void = {}
    var onOpenHistory: @MainActor () -> Void = {}

    private let panel: WidgetPanel
    private let host = WidgetHostView()
    private let model: WidgetModel
    private let session: DictationSession
    private let settings: AppSettings
    private var cancellables = Set<AnyCancellable>()
    private var anchor = CGPoint.zero
    private var fadeTask: Task<Void, Never>?
    private var shrinkTask: Task<Void, Never>?
    private var elapsedTimer: Timer?
    private var recordingStart: Date?
    private let margin: CGFloat = 8

    init(session: DictationSession, settings: AppSettings) {
        self.session = session
        self.settings = settings
        let model = WidgetModel()
        model.style = settings.widgetStyle
        model.glassOpacity = settings.glassOpacity
        model.keyLabel = settings.hotkey.keyLabel
        self.model = model
        panel = WidgetPanel(contentRect: NSRect(origin: .zero, size: WidgetLayout.envelope(model)))
        super.init()

        let hosting = NSHostingView(rootView: WidgetRootView(model: model))
        hosting.translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(hosting)
        NSLayoutConstraint.activate([
            hosting.leadingAnchor.constraint(equalTo: host.leadingAnchor),
            hosting.trailingAnchor.constraint(equalTo: host.trailingAnchor),
            hosting.topAnchor.constraint(equalTo: host.topAnchor),
            hosting.bottomAnchor.constraint(equalTo: host.bottomAnchor),
        ])
        panel.contentView = host

        host.onClick = { [weak self] in self?.session.toggleHandsFree() }
        host.onHoldBegan = { [weak self] in self?.session.startRecording(handsFree: false) }
        host.onHoldEnded = { [weak self] in self?.session.stopRecording() }
        host.onDragEnded = { [weak self] in self?.dragEnded() }
        host.onRightClick = { [weak self] event in self?.showMenu(event) }
        host.onHover = { [weak self] hovering in
            guard let self else { return }
            self.model.hovering = hovering
            self.fitPanel()
            self.updateOpacity()
        }

        restorePosition()
        session.$state.receive(on: RunLoop.main).sink { [weak self] state in
            MainActor.assumeIsolated { self?.apply(state) }
        }.store(in: &cancellables)
        session.recorder.$level.receive(on: RunLoop.main).sink { [weak self] level in
            MainActor.assumeIsolated { self?.model.pushLevel(level) }
        }.store(in: &cancellables)
        session.live.$text.receive(on: RunLoop.main).sink { [weak self] text in
            MainActor.assumeIsolated {
                guard let self else { return }
                let appeared = self.model.live.isEmpty != text.isEmpty
                self.model.live = text
                if appeared { self.fitPanel() }
            }
        }.store(in: &cancellables)
        settings.$widgetStyle.dropFirst().receive(on: RunLoop.main).sink { [weak self] style in
            MainActor.assumeIsolated {
                self?.model.style = style
                self?.fitPanel()
            }
        }.store(in: &cancellables)
        settings.$glassOpacity.dropFirst().receive(on: RunLoop.main).sink { [weak self] value in
            MainActor.assumeIsolated { self?.model.glassOpacity = value }
        }.store(in: &cancellables)
        settings.$hotkey.dropFirst().receive(on: RunLoop.main).sink { [weak self] key in
            MainActor.assumeIsolated { self?.model.keyLabel = key.keyLabel }
        }.store(in: &cancellables)
        settings.$fadeWhenIdle.dropFirst().receive(on: RunLoop.main).sink { [weak self] _ in
            MainActor.assumeIsolated { self?.updateOpacity() }
        }.store(in: &cancellables)
        settings.$widgetVisible.receive(on: RunLoop.main).sink { [weak self] visible in
            MainActor.assumeIsolated {
                guard let self else { return }
                if visible { self.panel.orderFrontRegardless() } else { self.panel.orderOut(nil) }
            }
        }.store(in: &cancellables)
        NotificationCenter.default.publisher(for: NSApplication.didChangeScreenParametersNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in MainActor.assumeIsolated { self?.screensChanged() } }
            .store(in: &cancellables)
        updateOpacity()
    }

    // MARK: State

    private func apply(_ state: DictationSession.State) {
        switch state {
        case .idle:
            model.phase = .idle
            stopElapsed()
        case .recording(let handsFree):
            model.target = session.targetAppName
            model.phase = .listening(handsFree: handsFree)
            startElapsed()
        case .loadingModel:
            model.phase = .processing(.loading)
            stopElapsed()
        case .transcribing:
            model.phase = .processing(.transcribing)
            stopElapsed()
        case .formatting:
            model.phase = .processing(.formatting)
        case .inserting:
            model.phase = .processing(.inserting)
        case .done(let preview):
            model.phase = .done(preview)
        case .failed(let message):
            model.phase = .failed(message)
        }
        fitPanel()
        updateOpacity()
    }

    private func startElapsed() {
        guard elapsedTimer == nil else { return }
        recordingStart = Date()
        model.elapsed = 0
        elapsedTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self, let start = self.recordingStart else { return }
                self.model.elapsed = Date().timeIntervalSince(start)
            }
        }
    }

    private func stopElapsed() {
        elapsedTimer?.invalidate()
        elapsedTimer = nil
        recordingStart = nil
    }

    // MARK: Geometry

    /// Sizes the panel for the widget's next shape. SwiftUI springs the shape itself, so the panel grows at once
    /// (around the anchor, where the shape stays put) and shrinks only after the spring has settled.
    private func fitPanel() {
        let target = WidgetLayout.envelope(model)
        let current = panel.frame.size
        let union = CGSize(width: max(current.width, target.width), height: max(current.height, target.height))
        shrinkTask?.cancel()
        if union != current { panel.setFrame(frame(for: union), display: true) }
        guard union != target else { return }
        shrinkTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(0.7))
            guard let self, !Task.isCancelled else { return }
            self.panel.setFrame(self.frame(for: WidgetLayout.envelope(self.model)), display: true)
        }
    }

    /// Puts the widget back at the bottom middle of the main display.
    func resetPosition() {
        anchor = Self.defaultAnchor()
        settings.widgetAnchor = nil
        panel.setFrame(frame(for: panel.frame.size), display: true, animate: true)
    }

    /// Where the widget is on screen, for cards that attach to it.
    var panelFrame: NSRect { panel.frame }

    private func restorePosition() {
        if let saved = settings.widgetAnchor, Self.screen(containing: saved) != nil {
            anchor = saved
        } else {
            anchor = Self.defaultAnchor()
        }
        panel.setFrame(frame(for: WidgetLayout.envelope(model)), display: false)
        if settings.widgetVisible { panel.orderFrontRegardless() }
    }

    private func dragEnded() {
        let dropped = panel.frame
        anchor = CGPoint(x: dropped.midX, y: dropped.minY)
        if Self.screen(containing: anchor) == nil { anchor = Self.defaultAnchor() }
        let fitted = frame(for: dropped.size)
        anchor = CGPoint(x: fitted.midX, y: fitted.minY)
        panel.setFrame(fitted, display: true, animate: true)
        settings.widgetAnchor = anchor
    }

    private func screensChanged() {
        if Self.screen(containing: anchor) == nil { anchor = Self.defaultAnchor() }
        panel.setFrame(frame(for: panel.frame.size), display: true, animate: false)
    }

    /// The frame for `size` around the anchor, kept inside the anchor's display.
    private func frame(for size: CGSize) -> NSRect {
        var frame = NSRect(x: anchor.x - size.width / 2, y: anchor.y, width: size.width, height: size.height)
        guard let visible = (Self.screen(containing: anchor) ?? NSScreen.screens.first)?.visibleFrame else { return frame }
        frame.origin.x = min(max(frame.minX, visible.minX + margin), visible.maxX - size.width - margin)
        frame.origin.y = min(max(frame.minY, visible.minY + margin), visible.maxY - size.height - margin)
        return frame
    }

    private static func screen(containing point: CGPoint) -> NSScreen? {
        // The anchor sits on the widget's bottom edge, so allow it to be a few points below a visible frame.
        NSScreen.screens.first { $0.visibleFrame.insetBy(dx: -2, dy: -16).contains(point) }
    }

    /// Bottom middle of the display with the menu bar, just above the Dock.
    private static func defaultAnchor() -> CGPoint {
        let visible = NSScreen.screens.first?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        return CGPoint(x: visible.midX, y: visible.minY + 4)
    }

    private func updateOpacity() {
        fadeTask?.cancel()
        let idle = model.phase == .idle
        if !idle || model.hovering || !settings.fadeWhenIdle {
            panel.animator().alphaValue = 1
            return
        }
        fadeTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(3))
            guard let self, !Task.isCancelled, self.model.phase == .idle, !self.model.hovering else { return }
            self.panel.animator().alphaValue = 0.55
        }
    }

    // MARK: Menu

    private func showMenu(_ event: NSEvent) {
        let menu = NSMenu()
        let header = NSMenuItem(title: "Next dictation mode", action: nil, keyEquivalent: "")
        header.isEnabled = false
        menu.addItem(header)
        let current = session.modeOverride ?? settings.defaultMode
        for mode in Mode.allCases {
            let mi = NSMenuItem(title: mode.displayName, action: #selector(pickMode(_:)), keyEquivalent: "")
            mi.target = self
            mi.representedObject = mode.rawValue
            mi.state = current == mode ? .on : .off
            menu.addItem(mi)
        }
        menu.addItem(.separator())
        let langHeader = NSMenuItem(title: "Language", action: nil, keyEquivalent: "")
        langHeader.isEnabled = false
        menu.addItem(langHeader)
        for (title, value) in [("Auto", "auto"), ("Українська", "uk"), ("English", "en")] {
            let mi = NSMenuItem(title: title, action: #selector(pickLanguage(_:)), keyEquivalent: "")
            mi.target = self
            mi.representedObject = value
            mi.state = settings.languageLock == value ? .on : .off
            menu.addItem(mi)
        }
        menu.addItem(.separator())
        if let last = session.lastRecord {
            let copy = NSMenuItem(title: "Copy last result", action: #selector(copyLast), keyEquivalent: "")
            copy.target = self
            copy.toolTip = last.formattedText
            menu.addItem(copy)
        }
        let history = NSMenuItem(title: "History…", action: #selector(openHistory), keyEquivalent: "")
        history.target = self
        menu.addItem(history)
        let settingsItem = NSMenuItem(title: "Settings…", action: #selector(openSettings), keyEquivalent: "")
        settingsItem.target = self
        menu.addItem(settingsItem)
        let hide = NSMenuItem(title: "Hide widget", action: #selector(hideWidget), keyEquivalent: "")
        hide.target = self
        menu.addItem(hide)
        NSMenu.popUpContextMenu(menu, with: event, for: host)
    }

    @objc private func pickMode(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let mode = Mode(rawValue: raw) else { return }
        session.modeOverride = mode == settings.defaultMode ? nil : mode
    }

    @objc private func pickLanguage(_ sender: NSMenuItem) {
        if let value = sender.representedObject as? String { settings.languageLock = value }
    }

    @objc private func copyLast() {
        guard let text = session.lastRecord?.formattedText else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    @objc private func openHistory() { onOpenHistory() }
    @objc private func openSettings() { onOpenSettings() }
    @objc private func hideWidget() { settings.widgetVisible = false }
}
