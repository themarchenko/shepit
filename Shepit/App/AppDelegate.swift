import AppKit
import Combine
import ShepitCore

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let settings = AppSettings.shared
    private var history: HistoryStore?
    private let engines = EngineManager()
    private let recorder = AudioRecorder()
    /// The agent that polishes text (Claude Code or Codex); nil when polishing is off or no agent is installed.
    private var polisher: (any ShepitCore.Formatter)?
    private var formatting: FormattingService!
    private var session: DictationSession!
    private var statusItem: StatusItemController!
    private var widget: WidgetController!
    private var hotkeys: HotkeyMonitor!
    private var settingsWindow: SettingsWindowController?
    private var historyWindow: HistoryWindowController?
    private var tourWindow: TourWindowController?
    private let whatsNew = WhatsNewController()
    private lazy var updates = UpdateChecker(settings: settings)
    private let updatePanel = UpdatePanelController()
    private var cancellables = Set<AnyCancellable>()

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            history = try HistoryStore(url: AppPaths.databaseURL)
        } catch {
            Log.store.error("history store unavailable: \(String(describing: error), privacy: .public)")
        }
        polisher = makePolisher()
        formatting = FormattingService(claude: polisher)
        session = DictationSession(recorder: recorder, engines: engines, formatting: formatting,
                                   inserter: PasteInserter(), history: history, settings: settings)

        statusItem = StatusItemController(session: session, settings: settings)
        statusItem.onOpenSettings = { [weak self] in self?.showSettings() }
        statusItem.onOpenHistory = { [weak self] in self?.showHistory() }
        statusItem.onQuit = { NSApp.terminate(nil) }
        statusItem.onCheckForUpdates = { [weak self] in self?.checkForUpdates() }
        statusItem.updates = updates
        updates.onFound = { [weak self] _ in self?.showUpdate() }
        updates.start()

        widget = WidgetController(session: session, settings: settings)
        widget.onOpenSettings = { [weak self] in self?.showSettings() }
        widget.onOpenHistory = { [weak self] in self?.showHistory() }

        hotkeys = HotkeyMonitor(choice: settings.hotkey)
        hotkeys.onPressBegan = { [weak self] in self?.hotkeyPressBegan() }
        hotkeys.onPressEnded = { [weak self] in self?.session.stopRecording() }
        hotkeys.onShortTap = { [weak self] in self?.hotkeyShortTap() }
        hotkeys.onDoubleTap = { [weak self] in self?.session.continueHandsFree() }
        hotkeys.onEscape = { [weak self] in self?.escapePressed() }

        observeSettings()
        // The welcome tour asks for the permissions itself; until it is done, nothing prompts at launch.
        let firstRun = !settings.onboardingCompleted
        Task { await self.requestPermissions(prompt: !firstRun) }
        if firstRun {
            Task {
                try? await Task.sleep(for: .seconds(0.6))
                self.showWelcomeTour()
            }
        } else {
            Task {
                try? await Task.sleep(for: .seconds(2))
                self.showWhatsNewIfUnseen()
            }
        }
        session.$state.receive(on: RunLoop.main).sink { [weak self] state in
            MainActor.assumeIsolated {
                if state.isRecording, let self, self.whatsNew.isShown { self.whatsNew.dismiss() }
            }
        }.store(in: &cancellables)
        whatsNew.onClose = { [weak self] in
            guard let self else { return }
            self.settings.whatsNewSeen = WhatsNew.current
        }
        Task {
            // Parakeet loads in under a second and serves dictations until a slower engine is ready; it stays
            // loaded afterwards for the live preview.
            await self.engines.setKeepsPreviewEngine(self.settings.livePreview)
            if self.settings.engine != .parakeetV3 { await self.engines.preload(.parakeetV3) }
            await self.engines.preload(self.settings.engine)
        }
        Task { await self.formatting.warmUp() }
        Task {
            await self.formatting.setFastPath(maxWords: self.settings.fastPathMaxWords)
        }
        Log.ui.notice("Shepit launched")
    }

    func applicationWillTerminate(_ notification: Notification) {
        hotkeys.stop()
        let polisher = self.polisher
        Task { await Self.shutdown(polisher) }
    }

    // MARK: Hotkey semantics

    private func hotkeyPressBegan() {
        switch session.state {
        case .recording(handsFree: true):
            session.stopRecording()
        case .recording(handsFree: false):
            break
        default:
            session.startRecording(handsFree: false)
        }
    }

    /// A tap too short to be a dictation. Cancels the recording the press started, unless it was stopping hands-free.
    private func hotkeyShortTap() {
        if case .recording(handsFree: false) = session.state {
            session.cancel()
        }
    }

    private func escapePressed() {
        if session.state.isRecording || session.state.isBusy { session.cancel() }
    }

    // MARK: Setup

    /// The selected polishing agent, or nil when polishing is off or the agent is not installed (the text then
    /// goes in as heard, with local cleanup and dictionary fixes).
    private func makePolisher() -> (any ShepitCore.Formatter)? {
        let systemPrompt = PromptBuilder.systemPrompt(speakerNotes: settings.speakerNotes)
        switch settings.polishing {
        case .off:
            return nil
        case .claudeCode:
            guard let executable = settings.resolvedClaudeExecutable else {
                Log.formatter.error("claude executable not found; polishing off")
                return nil
            }
            let config = ClaudeWorkerConfig(executable: executable, model: settings.claudeModel.rawValue,
                                            systemPrompt: systemPrompt, workDirectory: AppPaths.claudeWorkDirectory)
            return ClaudeCodeWorker(config: config)
        case .codex:
            guard let executable = settings.resolvedCodexExecutable else {
                Log.formatter.error("codex executable not found; polishing off")
                return nil
            }
            // An empty model uses whatever the user's Codex is set to, which their plan is known to allow.
            let config = CodexConfig(executable: executable, model: "", systemPrompt: systemPrompt,
                                     workDirectory: AppPaths.codexWorkDirectory)
            return CodexFormatter(config: config)
        }
    }

    private static func shutdown(_ polisher: (any ShepitCore.Formatter)?) async {
        if let claude = polisher as? ClaudeCodeWorker { await claude.shutdown() }
        if let codex = polisher as? CodexFormatter { await codex.shutdown() }
    }

    private func observeSettings() {
        settings.$hotkey.dropFirst().sink { [weak self] choice in
            MainActor.assumeIsolated { self?.hotkeys.update(choice: choice) }
        }.store(in: &cancellables)

        settings.$engine.dropFirst().removeDuplicates().sink { [weak self] engine in
            MainActor.assumeIsolated {
                guard let self else { return }
                Task { await self.engines.preload(engine) }
            }
        }.store(in: &cancellables)

        // Speaker notes live in the system prompt, so editing them respawns the agent (debounced while typing).
        Publishers.CombineLatest4(settings.$polishing, settings.$claudeModel, settings.$speakerNotes,
                                  Publishers.CombineLatest(settings.$claudeExecutable, settings.$codexExecutable))
            .dropFirst()
            .debounce(for: .seconds(1.5), scheduler: RunLoop.main)
            .sink { [weak self] _, _, _, _ in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    let old = self.polisher
                    self.polisher = self.makePolisher()
                    let new = self.polisher
                    let formatting = self.formatting!
                    Task {
                        await Self.shutdown(old)
                        await formatting.setClaude(new)
                        await formatting.warmUp()
                    }
                }
            }.store(in: &cancellables)

        settings.$livePreview.dropFirst().removeDuplicates().sink { [weak self] on in
            MainActor.assumeIsolated {
                guard let self else { return }
                let engines = self.engines
                Task { await engines.setKeepsPreviewEngine(on) }
            }
        }.store(in: &cancellables)

        settings.$fastPathMaxWords.dropFirst().sink { [weak self] words in
            MainActor.assumeIsolated {
                guard let self else { return }
                let formatting = self.formatting!
                Task { await formatting.setFastPath(maxWords: words) }
            }
        }.store(in: &cancellables)
    }

    /// Starts the hotkey monitor and prepares the microphone. With `prompt` it also shows the system prompts for
    /// what is missing; during the welcome tour the tour's buttons do that instead.
    private func requestPermissions(prompt: Bool) async {
        let mic = prompt ? await AudioRecorder.requestPermission() : AudioRecorder.permissionGranted
        if mic {
            recorder.prepare()
        } else {
            Log.audio.error("microphone permission not granted")
        }
        if prompt, !ContextProbe.isAccessibilityTrusted {
            ContextProbe.requestAccessibility()
        }
        hotkeys.start()
        Log.ui.notice("microphone granted: \(mic, privacy: .public), accessibility trusted: \(ContextProbe.isAccessibilityTrusted, privacy: .public)")
        // Global key monitors installed before Accessibility was granted stay deaf; re-arm them once it is.
        if !ContextProbe.isAccessibilityTrusted {
            Task { [weak self] in
                while let self, !ContextProbe.isAccessibilityTrusted {
                    try? await Task.sleep(for: .seconds(3))
                    if ContextProbe.isAccessibilityTrusted {
                        self.hotkeys.stop()
                        self.hotkeys.start()
                        Log.ui.notice("accessibility granted, hotkey monitor re-armed")
                    }
                }
            }
        }
    }

    // MARK: Windows

    func showSettings(tab: SettingsWindowController.Tab? = nil) {
        if settingsWindow == nil {
            let actions = SettingsActions(
                resetWidgetPosition: { [weak self] in self?.widget.resetPosition() },
                showWhatsNew: { [weak self] in self?.showWhatsNew(force: true) },
                showWelcomeTour: { [weak self] in self?.showWelcomeTour() },
                showUpdate: { [weak self] in self?.showUpdate() })
            settingsWindow = SettingsWindowController(settings: settings, updates: updates, actions: actions)
        }
        settingsWindow?.show(tab: tab)
    }

    func showWelcomeTour() {
        if tourWindow == nil {
            let recorder = self.recorder
            tourWindow = TourWindowController(settings: settings) {
                let granted = await AudioRecorder.requestPermission()
                if granted { recorder.prepare() }
                return granted
            }
            tourWindow?.onFinish = { [weak self] in
                guard let self else { return }
                let firstRun = !self.settings.onboardingCompleted
                self.settings.onboardingCompleted = true
                self.tourWindow = nil
                // A new user needs no release notes; someone who dictated before the tour existed gets them.
                if firstRun, ((try? self.history?.recent(limit: 1)) ?? []).isEmpty {
                    self.settings.whatsNewSeen = WhatsNew.current
                } else {
                    self.showWhatsNewIfUnseen()
                }
            }
        }
        tourWindow?.show()
    }

    private func showWhatsNewIfUnseen() {
        guard WhatsNew.unseen(after: settings.whatsNewSeen) != nil else { return }
        showWhatsNew(force: false)
    }

    private func showWhatsNew(force: Bool) {
        let release = force ? WhatsNew.releases.first : WhatsNew.unseen(after: settings.whatsNewSeen)
        guard let release else { return }
        if !settings.widgetVisible { settings.widgetVisible = true }
        whatsNew.show(version: release.version, items: release.items, above: widget.panelFrame) { [weak self] in
            self?.showSettings(tab: .appearance)
        }
    }

    func showUpdate() {
        updatePanel.show(updates)
    }

    /// From the menu: the card shows the check, then the result.
    func checkForUpdates() {
        if case .available = updates.state {
            showUpdate()
            return
        }
        showUpdate()
        Task { await updates.check(userInitiated: true) }
    }

    func showHistory() {
        if historyWindow == nil { historyWindow = HistoryWindowController(history: history) }
        historyWindow?.show()
    }
}
