import AppKit
import Combine
import ShepitCore

@MainActor
final class StatusItemController: NSObject, NSMenuDelegate {
    var onOpenSettings: @MainActor () -> Void = {}
    var onOpenHistory: @MainActor () -> Void = {}
    var onOpenDoctor: @MainActor () -> Void = {}
    /// Asked when the icon is redrawn and when the menu opens; true marks the icon with a warning.
    var doctorNeedsAttention: @MainActor () -> Bool = { false }
    var onQuit: @MainActor () -> Void = {}
    var onCheckForUpdates: @MainActor () -> Void = {}
    var updates: UpdateChecker?

    private let item: NSStatusItem
    private let session: DictationSession
    private let settings: AppSettings
    private let menu = NSMenu()
    private var cancellables = Set<AnyCancellable>()

    init(session: DictationSession, settings: AppSettings) {
        self.session = session
        self.settings = settings
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        menu.delegate = self
        item.menu = menu
        updateIcon(for: session.state)
        session.$state.receive(on: RunLoop.main).sink { [weak self] state in
            MainActor.assumeIsolated { self?.updateIcon(for: state) }
        }.store(in: &cancellables)
        // A permission fixed in System Settings is noticed when the user comes back to the app.
        NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification).sink { [weak self] _ in
            MainActor.assumeIsolated { self?.refreshIcon() }
        }.store(in: &cancellables)
    }

    func refreshIcon() { updateIcon(for: session.state) }

    /// At rest the menu bar shows the brand glyph; states that need attention keep their symbols.
    private func updateIcon(for state: DictationSession.State) {
        let name: String
        var color: NSColor? = nil
        switch state {
        case .idle, .done:
            if doctorNeedsAttention(),
               let warn = NSImage(systemSymbolName: "exclamationmark.triangle.fill", accessibilityDescription: String(localized: "Shepit needs attention")) {
                item.button?.image = warn.withSymbolConfiguration(.init(paletteColors: [.systemOrange]))
                item.button?.image?.isTemplate = false
                return
            }
            let glyph = NSImage(named: "MenuBarIcon")
            glyph?.isTemplate = true
            glyph?.accessibilityDescription = "Shepit"
            item.button?.image = glyph
            return
        case .reviewing: name = "text.cursor"
        case .recording: name = "record.circle.fill"; color = .systemRed
        case .loadingModel, .transcribing, .formatting, .inserting: name = "ellipsis.circle"
        case .failed: name = "exclamationmark.circle"; color = .systemOrange
        }
        let image = NSImage(systemSymbolName: name, accessibilityDescription: "Shepit")
        if let color, let image {
            item.button?.image = image.withSymbolConfiguration(.init(paletteColors: [color]))
        } else {
            item.button?.image = image
        }
        item.button?.image?.isTemplate = color == nil
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        // The icon is redrawn on state changes only, so a permission fixed in System Settings clears the marker here.
        updateIcon(for: session.state)
        menu.removeAllItems()
        let status = NSMenuItem(title: statusText, action: nil, keyEquivalent: "")
        status.isEnabled = false
        menu.addItem(status)
        menu.addItem(.separator())

        let handsFree = NSMenuItem(title: session.state.isRecording ? String(localized: "Stop dictation") : String(localized: "Start hands-free dictation"),
                                   action: #selector(toggleHandsFree), keyEquivalent: "")
        handsFree.target = self
        menu.addItem(handsFree)

        let modeMenu = NSMenu()
        for mode in Mode.allCases {
            let mi = NSMenuItem(title: mode.displayName, action: #selector(selectMode(_:)), keyEquivalent: "")
            mi.target = self
            mi.representedObject = mode.rawValue
            mi.state = settings.defaultMode == mode ? .on : .off
            modeMenu.addItem(mi)
        }
        let modeItem = NSMenuItem(title: String(localized: "Default mode"), action: nil, keyEquivalent: "")
        modeItem.submenu = modeMenu
        menu.addItem(modeItem)

        let engineMenu = NSMenu()
        for engine in EngineID.allCases {
            let mi = NSMenuItem(title: engine.displayName, action: #selector(selectEngine(_:)), keyEquivalent: "")
            mi.target = self
            mi.representedObject = engine.rawValue
            mi.state = settings.engine == engine ? .on : .off
            engineMenu.addItem(mi)
        }
        let engineItem = NSMenuItem(title: String(localized: "Recognizer"), action: nil, keyEquivalent: "")
        engineItem.submenu = engineMenu
        menu.addItem(engineItem)

        let widgetItem = NSMenuItem(title: String(localized: "Show widget"), action: #selector(toggleWidget), keyEquivalent: "")
        widgetItem.target = self
        widgetItem.state = settings.widgetVisible ? .on : .off
        menu.addItem(widgetItem)
        menu.addItem(.separator())

        let historyItem = NSMenuItem(title: String(localized: "History…"), action: #selector(openHistory), keyEquivalent: "")
        historyItem.target = self
        menu.addItem(historyItem)
        let doctorItem = NSMenuItem(title: String(localized: "Doctor…"), action: #selector(openDoctor), keyEquivalent: "")
        doctorItem.target = self
        menu.addItem(doctorItem)
        let settingsItem = NSMenuItem(title: String(localized: "Settings…"), action: #selector(openSettings), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)
        var updateTitle = String(localized: "Check for Updates…")
        if case .available(let release) = updates?.state { updateTitle = String(localized: "Update to \(release.version)…") }
        let updateItem = NSMenuItem(title: updateTitle, action: #selector(checkForUpdates), keyEquivalent: "")
        updateItem.target = self
        menu.addItem(updateItem)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: String(localized: "Quit Shepit"), action: #selector(quit), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
    }

    private var statusText: String {
        switch session.state {
        case .idle: return String(localized: "Idle · hold \(settings.hotkey.shortName) to dictate")
        case .recording(let handsFree): return handsFree ? String(localized: "Listening (hands-free)") : String(localized: "Listening")
        case .reviewing: return String(localized: "Review: Enter pastes, Esc cancels")
        case .loadingModel: return String(localized: "Loading recognizer…")
        case .transcribing: return String(localized: "Transcribing…")
        case .formatting: return String(localized: "Formatting…")
        case .inserting: return String(localized: "Inserting…")
        case .done: return String(localized: "Inserted")
        case .failed(let message): return String(localized: "Failed: \(message)")
        }
    }

    @objc private func toggleHandsFree() { session.toggleHandsFree() }
    @objc private func selectMode(_ sender: NSMenuItem) {
        if let raw = sender.representedObject as? String, let mode = Mode(rawValue: raw) { settings.defaultMode = mode }
    }
    @objc private func selectEngine(_ sender: NSMenuItem) {
        if let raw = sender.representedObject as? String, let engine = EngineID(rawValue: raw) { settings.engine = engine }
    }
    @objc private func toggleWidget() { settings.widgetVisible.toggle() }
    @objc private func openHistory() { onOpenHistory() }
    @objc private func openDoctor() { onOpenDoctor() }
    @objc private func openSettings() { onOpenSettings() }
    @objc private func checkForUpdates() { onCheckForUpdates() }
    @objc private func quit() { onQuit() }
}
