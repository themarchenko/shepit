import AppKit
import Combine
import ShepitCore

@MainActor
final class StatusItemController: NSObject, NSMenuDelegate {
    var onOpenSettings: @MainActor () -> Void = {}
    var onOpenHistory: @MainActor () -> Void = {}
    var onQuit: @MainActor () -> Void = {}

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
    }

    /// At rest the menu bar shows the brand glyph; states that need attention keep their symbols.
    private func updateIcon(for state: DictationSession.State) {
        let name: String
        var color: NSColor? = nil
        switch state {
        case .idle, .done:
            let glyph = NSImage(named: "MenuBarIcon")
            glyph?.isTemplate = true
            glyph?.accessibilityDescription = "Shepit"
            item.button?.image = glyph
            return
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
        menu.removeAllItems()
        let status = NSMenuItem(title: statusText, action: nil, keyEquivalent: "")
        status.isEnabled = false
        menu.addItem(status)
        menu.addItem(.separator())

        let handsFree = NSMenuItem(title: session.state.isRecording ? "Stop dictation" : "Start hands-free dictation",
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
        let modeItem = NSMenuItem(title: "Default mode", action: nil, keyEquivalent: "")
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
        let engineItem = NSMenuItem(title: "Recognizer", action: nil, keyEquivalent: "")
        engineItem.submenu = engineMenu
        menu.addItem(engineItem)

        let widgetItem = NSMenuItem(title: "Show widget", action: #selector(toggleWidget), keyEquivalent: "")
        widgetItem.target = self
        widgetItem.state = settings.widgetVisible ? .on : .off
        menu.addItem(widgetItem)
        menu.addItem(.separator())

        let historyItem = NSMenuItem(title: "History…", action: #selector(openHistory), keyEquivalent: "")
        historyItem.target = self
        menu.addItem(historyItem)
        let settingsItem = NSMenuItem(title: "Settings…", action: #selector(openSettings), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Shepit", action: #selector(quit), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
    }

    private var statusText: String {
        switch session.state {
        case .idle: return "Idle · hold \(settings.hotkey.displayName.components(separatedBy: " (").first ?? "key") to dictate"
        case .recording(let handsFree): return handsFree ? "Listening (hands-free)" : "Listening"
        case .loadingModel: return "Loading recognizer…"
        case .transcribing: return "Transcribing…"
        case .formatting: return "Formatting…"
        case .inserting: return "Inserting…"
        case .done: return "Inserted"
        case .failed(let message): return "Failed: \(message)"
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
    @objc private func openSettings() { onOpenSettings() }
    @objc private func quit() { onQuit() }
}
