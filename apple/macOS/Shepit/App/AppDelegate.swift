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
    private var doctorWindow: DoctorWindowController?
    /// Doctor opens itself at most once per launch, so a user who closes it is not nagged.
    private var doctorAutoOpened = false
    private var tourWindow: TourWindowController?
    private var calibrationWindow: CalibrationWindowController?
    private let whatsNew = WhatsNewController()
    /// Same card as What's New, but it never writes whatsNewSeen: the tip has its own flag.
    private let reviewTip = WhatsNewController()
    /// Set when a dictation was pasted without review; the tip waits for the widget to go idle.
    private var reviewTipPending = false
    /// Set when the calibration offer is waiting for the What's new card to close.
    private var offerCalibrationAfterWhatsNew = false
    private lazy var updates = UpdateChecker(settings: settings)
    private let updatePanel = UpdatePanelController()
    private var review: ReviewPanelController!
    private var cancellables = Set<AnyCancellable>()

    func applicationDidFinishLaunching(_ notification: Notification) {
        // An accessory app never shows a menu bar, but key equivalents reach the key window through the main
        // menu while Shepit is active, which is what makes Cmd-C/V/A/Z work in Settings fields and the tour.
        NSApp.mainMenu = MainMenu.make()
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
        statusItem.onOpenDoctor = { [weak self] in self?.showDoctor() }
        statusItem.doctorNeedsAttention = { [weak self] in
            // The tour asks for these permissions itself.
            guard self?.settings.onboardingCompleted == true else { return false }
            return Doctor.requiredProblem(accessibilityTrusted: SystemProbe.accessibilityTrusted,
                                   microphone: AudioRecorder.microphoneAccess)
        }
        statusItem.onQuit = { NSApp.terminate(nil) }
        statusItem.onCheckForUpdates = { [weak self] in self?.checkForUpdates() }
        statusItem.updates = updates
        updates.onFound = { [weak self] _ in self?.showUpdate() }
        updates.start()

        widget = WidgetController(session: session, settings: settings)
        widget.onOpenSettings = { [weak self] in self?.showSettings() }
        widget.onOpenHistory = { [weak self] in self?.showHistory() }
        widget.onOpenDoctor = { [weak self] in self?.showDoctor() }
        review = ReviewPanelController(session: session, widget: widget)

        hotkeys = HotkeyMonitor(choice: settings.hotkey)
        hotkeys.onPressBegan = { [weak self] shift in self?.hotkeyPressBegan(shift: shift) }
        hotkeys.onPressEnded = { [weak self] shift in self?.session.stopRecording(review: shift) }
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
                if self.showWhatsNewIfUnseen() {
                    self.offerCalibrationAfterWhatsNew = true // offered when the card closes
                } else {
                    try? await Task.sleep(for: .seconds(1.5))
                    self.offerCalibrationIfDue()
                }
            }
        }
        session.$state.receive(on: RunLoop.main).sink { [weak self] state in
            MainActor.assumeIsolated {
                guard let self else { return }
                if state.isRecording || state.isReviewing {
                    if self.whatsNew.isShown { self.whatsNew.dismiss() }
                    if self.reviewTip.isShown { self.reviewTip.dismiss() }
                }
                self.updateReviewTip(for: state)
                self.calibrationWindow?.model.sessionChanged(state)
            }
        }.store(in: &cancellables)
        whatsNew.onClose = { [weak self] in
            guard let self else { return }
            self.settings.whatsNewSeen = WhatsNew.current
            if self.offerCalibrationAfterWhatsNew {
                self.offerCalibrationAfterWhatsNew = false
                Task {
                    try? await Task.sleep(for: .seconds(0.8))
                    self.offerCalibrationIfDue()
                }
            }
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
        // A quit during review still writes the history row, so the text is recoverable.
        session.discardReview()
        let polisher = self.polisher
        Task { await Self.shutdown(polisher) }
    }

    // MARK: Hotkey semantics

    private func hotkeyPressBegan(shift: Bool) {
        switch session.state {
        case .recording(handsFree: true):
            session.stopRecording(review: shift)
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
        if session.state.isReviewing {
            session.discardReview()
        } else if session.state.isRecording || session.state.isBusy {
            session.cancel()
        }
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
        // Doctor explains a missing permission and can clear a stale one, which the bare system prompt cannot.
        if prompt, Doctor.shouldAutoOpen(onboardingCompleted: settings.onboardingCompleted, tourOpen: tourWindow != nil,
                                         alreadyOpened: doctorAutoOpened,
                                         accessibilityTrusted: ContextProbe.isAccessibilityTrusted,
                                         microphone: AudioRecorder.microphoneAccess) {
            doctorAutoOpened = true
            showDoctor()
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
                showUpdate: { [weak self] in self?.showUpdate() },
                showDoctor: { [weak self] in self?.showDoctor() },
                showCalibration: { [weak self] in self?.showCalibration() })
            settingsWindow = SettingsWindowController(settings: settings, updates: updates, actions: actions)
        }
        settingsWindow?.show(tab: tab)
    }

    func showWelcomeTour() {
        if tourWindow == nil {
            let recorder = self.recorder
            tourWindow = TourWindowController(settings: settings, requestMicrophone: {
                let granted = await AudioRecorder.requestPermission()
                if granted { recorder.prepare() }
                return granted
            }, openDoctor: { [weak self] in self?.showDoctor() })
            tourWindow?.onFinish = { [weak self] in
                guard let self else { return }
                let firstRun = !self.settings.onboardingCompleted
                self.settings.onboardingCompleted = true
                self.statusItem.refreshIcon()
                self.tourWindow = nil
                // A new user needs no release notes; someone who dictated before the tour existed gets them.
                if firstRun, ((try? self.history?.recent(limit: 1)) ?? []).isEmpty {
                    self.settings.whatsNewSeen = WhatsNew.current
                } else if self.showWhatsNewIfUnseen() {
                    self.offerCalibrationAfterWhatsNew = true
                    return
                }
                Task {
                    try? await Task.sleep(for: .seconds(0.8))
                    self.offerCalibrationIfDue()
                }
            }
        }
        tourWindow?.show()
    }

    // MARK: Voice calibration

    private func makeCalibrationAgent() -> CalibrationAgent? {
        CalibrationAgents.make(provider: settings.polishing, claudeExecutable: settings.resolvedClaudeExecutable,
                               claudeModel: settings.claudeModel.rawValue, codexExecutable: settings.resolvedCodexExecutable)
    }

    /// Opens the wizard once for someone whose speaker notes are still unset. The flag is set before it opens, so
    /// closing the window or quitting during it never brings it back.
    private func offerCalibrationIfDue() {
        guard CalibrationOffer.shouldAutoOpen(
            offered: settings.calibrationOffered, onboardingCompleted: settings.onboardingCompleted,
            tourOpen: tourWindow != nil, doctorOpened: doctorAutoOpened,
            requiredProblem: Doctor.requiredProblem(accessibilityTrusted: SystemProbe.accessibilityTrusted,
                                                    microphone: AudioRecorder.microphoneAccess),
            speakerNotes: settings.speakerNotes, agentAvailable: makeCalibrationAgent() != nil) else { return }
        settings.calibrationOffered = true
        showCalibration()
    }

    func showCalibration() {
        if calibrationWindow == nil {
            let session = self.session!
            let model = CalibrationModel(
                settings: settings,
                setSink: { sink in session.captureSink = sink },
                startRecording: {
                    guard !session.state.isRecording, !session.state.isBusy, !session.state.isReviewing else { return }
                    session.startRecording(handsFree: false)
                },
                stopRecording: { session.stopRecording(review: false) },
                cancelRecording: { session.cancel() },
                makeCoach: { [weak self] policy in
                    CalibrationCoach(agent: self?.makeCalibrationAgent(), policy: policy)
                })
            let controller = CalibrationWindowController(model: model)
            controller.onClose = { [weak self] in self?.calibrationWindow = nil }
            calibrationWindow = controller
        }
        calibrationWindow?.show()
    }

    /// True when the card was shown, so the caller can wait for it to close before offering anything else.
    @discardableResult
    private func showWhatsNewIfUnseen() -> Bool {
        guard WhatsNew.unseen(after: settings.whatsNewSeen) != nil else { return false }
        showWhatsNew(force: false)
        return whatsNew.isShown
    }

    private func updateReviewTip(for state: DictationSession.State) {
        if state.isReviewing {
            // Someone found review on their own, so the tip has nothing left to say.
            if !settings.reviewTipShown { settings.reviewTipShown = true }
            reviewTipPending = false
        } else if case .done = state {
            if let insert = session.lastInsert,
               ReviewTip.shouldShow(tipShown: settings.reviewTipShown, reviewBeforePaste: settings.reviewBeforePaste,
                                    onboardingCompleted: settings.onboardingCompleted, insert: insert) {
                reviewTipPending = true
            }
        } else if case .idle = state, reviewTipPending {
            reviewTipPending = false
            // A replayed tour has its own Shift line; keep the flag for the next plain dictation.
            if !whatsNew.isShown && !settings.reviewTipShown && tourWindow == nil && calibrationWindow == nil { showReviewTip() }
        }
    }

    private func showReviewTip() {
        // Set first so a crash or a quit while the card is up cannot make it appear twice.
        settings.reviewTipShown = true
        let item = WhatsNew.Item(
            symbol: "text.cursor", title: "Check the text before it is pasted",
            detail: "Hold Shift as you let go of \(settings.hotkey.keyLabel) to see and edit the text first. Enter pastes, Esc cancels. Settings → General can do this for every dictation.")
        reviewTip.show(heading: "Tip", version: nil, items: [item], secondaryTitle: "Open Settings",
                       above: widget.restingFrame) { [weak self] in
            self?.showSettings(tab: .general)
        }
    }

    private func showWhatsNew(force: Bool) {
        let release = force ? WhatsNew.releases.first : WhatsNew.unseen(after: settings.whatsNewSeen)
        guard let release else { return }
        if !settings.widgetVisible { settings.widgetVisible = true }
        whatsNew.show(heading: "New in Shepit", version: release.version, items: release.items,
                      secondaryTitle: "Open Appearance", above: widget.panelFrame) { [weak self] in
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

    func showDoctor() {
        if doctorWindow == nil {
            let recorder = self.recorder
            let model = DoctorModel(
                settings: settings, engines: engines, formatting: formatting, session: session,
                requestMicrophone: {
                    let granted = await AudioRecorder.requestPermission()
                    if granted { recorder.prepare() }
                    return granted
                },
                openSettings: { [weak self] tab in self?.showSettings(tab: tab) })
            model.onChange = { [weak self] in self?.statusItem.refreshIcon() }
            doctorWindow = DoctorWindowController(model: model)
        }
        doctorWindow?.show()
    }

    func showHistory() {
        if historyWindow == nil {
            historyWindow = HistoryWindowController(history: history, onCleared: { [weak self] in
                self?.session.forgetLastRecord()
            })
        }
        historyWindow?.show()
    }
}
