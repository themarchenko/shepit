import AppKit
import SwiftUI
import ShepitCore

/// Live state behind the Doctor window. Probes run once a second while the window is open, so a row turns
/// green by itself when the user fixes something in System Settings.
@MainActor
final class DoctorModel: ObservableObject {
    @Published private(set) var checks: [DoctorCheck]
    @Published private(set) var summary: String

    private struct Live {
        let settings: AppSettings
        let engines: EngineManager
        let formatting: FormattingService
        let session: DictationSession
        let requestMicrophone: () async -> Bool
        let openSettings: (SettingsWindowController.Tab) -> Void
    }

    private let live: Live?
    private var resetState: AccessibilityResetState = .idle
    private var agentTest: AgentTestState = .notRun
    private var timer: Timer?
    private var generation = 0
    /// Called after the rows changed or the window stopped refreshing, so the menu bar marker can follow.
    var onChange: (() -> Void)?

    /// Fixed rows, for rendering without probing the machine.
    init(checks: [DoctorCheck]) {
        self.checks = checks
        summary = Doctor.summary(checks)
        live = nil
    }

    init(settings: AppSettings, engines: EngineManager, formatting: FormattingService, session: DictationSession,
         requestMicrophone: @escaping () async -> Bool,
         openSettings: @escaping (SettingsWindowController.Tab) -> Void) {
        checks = []
        summary = ""
        live = Live(settings: settings, engines: engines, formatting: formatting, session: session,
                    requestMicrophone: requestMicrophone, openSettings: openSettings)
        refresh()
    }

    func start() {
        guard live != nil, timer == nil else { return }
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        onChange?()
    }

    /// Whether the agent test may run: it would compete with a dictation for the same agent.
    var agentTestEnabled: Bool {
        guard let session = live?.session else { return false }
        return !(session.state.isRecording || session.state.isBusy || session.state.isReviewing)
    }

    private func refresh() {
        guard let live else { return }
        let settings = live.settings
        let executable: String?
        switch settings.polishing {
        case .off: executable = nil
        case .claudeCode: executable = settings.resolvedClaudeExecutable
        case .codex: executable = settings.resolvedCodexExecutable
        }
        let secure = SystemProbe.secureInput()
        let base = DoctorInputs(
            accessibilityTrusted: SystemProbe.accessibilityTrusted, accessibilityReset: resetState,
            microphone: AudioRecorder.microphoneAccess, listenEventAccess: SystemProbe.listenEventAccess,
            hotkey: settings.hotkey, fnUsageType: SystemProbe.fnUsageType(), engine: settings.engine,
            modelOnDisk: EngineFactory.modelsOnDisk(settings.engine), modelState: .idle,
            polishing: settings.polishing, agentExecutable: executable, agentTest: agentTest,
            secureInputActive: secure.active, secureInputHolder: secure.holder)
        let engines = live.engines
        let engine = settings.engine
        generation += 1
        let mine = generation
        Task {
            var inputs = base
            inputs.modelState = await engines.loadState(of: engine)
            // A slower, older probe must not overwrite a newer one.
            guard mine == self.generation else { return }
            let rows = Doctor.evaluate(inputs)
            if rows != self.checks {
                self.checks = rows
                self.summary = Doctor.summary(rows)
                self.onChange?()
            }
        }
    }

    func perform(_ fix: DoctorFix) {
        guard let live else { return }
        switch fix {
        case .resetAccessibility:
            guard resetState != .running else { return }
            resetState = .running
            refresh()
            Task {
                let ok = await AccessibilityReset.perform(bundleID: Bundle.main.bundleIdentifier)
                self.resetState = ok ? .done : .failed
                // Asking again puts Shepit back in the list; the list opens either way so the manual steps apply.
                openAccessibilitySettings()
                self.refresh()
            }
        case .requestMicrophone:
            Task {
                _ = await live.requestMicrophone()
                self.refresh()
            }
        case .openMicrophoneSettings:
            openPrivacy("Privacy_Microphone")
        case .openKeyboardSettings:
            if let url = URL(string: "x-apple.systempreferences:com.apple.Keyboard-Settings.extension") {
                NSWorkspace.shared.open(url)
            }
        case .openInputMonitoring:
            SystemProbe.requestListenEventAccess()
            openPrivacy("Privacy_ListenEvent")
        case .downloadModel:
            let engines = live.engines
            let engine = live.settings.engine
            Task {
                await engines.retry(engine)
                self.refresh()
            }
        case .openPolishingSettings:
            live.openSettings(.polishing)
        case .testAgent:
            guard agentTestEnabled, agentTest != .running else { return }
            agentTest = .running
            refresh()
            let formatting = live.formatting
            Task {
                let result = await formatting.testAgent()
                switch result {
                case .passed: self.agentTest = .passed
                case .noAgent: self.agentTest = .failed(kind: "notRunning")
                case .failed(let kind): self.agentTest = .failed(kind: kind)
                }
                self.refresh()
            }
        }
    }
}

struct DoctorView: View {
    @ObservedObject var model: DoctorModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "stethoscope").font(.system(size: 18))
                VStack(alignment: .leading, spacing: 1) {
                    Text("Shepit Doctor").font(.system(size: 16, weight: .semibold))
                    Text(model.summary).font(.system(size: 12.5)).foregroundStyle(WidgetPalette.text.opacity(0.6))
                }
                Spacer()
            }
            .padding(.horizontal, 22).padding(.top, 38).padding(.bottom, 14)
            ScrollView {
                VStack(spacing: 10) {
                    ForEach(model.checks) { row($0) }
                }
                .padding(.horizontal, 22).padding(.bottom, 22)
            }
        }
        .foregroundStyle(WidgetPalette.text)
        .frame(minWidth: 600, maxWidth: .infinity, minHeight: 560, maxHeight: .infinity, alignment: .top)
        .background(Color.black.opacity(0.55))
        .environment(\.colorScheme, .dark)
    }

    private func row(_ c: DoctorCheck) -> some View {
        HStack(alignment: .top, spacing: 12) {
            statusIcon(c.status).frame(width: 22, height: 22)
            VStack(alignment: .leading, spacing: 3) {
                Text(c.title).font(.system(size: 13, weight: .medium))
                Text(c.detail)
                    .font(.system(size: 12.5))
                    .foregroundStyle(WidgetPalette.text.opacity(0.6))
                    .fixedSize(horizontal: false, vertical: true)
                if !c.steps.isEmpty {
                    VStack(alignment: .leading, spacing: 3) {
                        ForEach(Array(c.steps.enumerated()), id: \.offset) { index, step in
                            HStack(alignment: .firstTextBaseline, spacing: 6) {
                                Text(verbatim: "\(index + 1).").monospacedDigit()
                                Text(step).fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                    .font(.system(size: 12.5))
                    .foregroundStyle(WidgetPalette.text.opacity(0.85))
                    .padding(.top, 4)
                }
            }
            Spacer(minLength: 8)
            if let fix = c.fix, let title = c.fixTitle, c.status != .checking {
                Button(title) { model.perform(fix) }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(fix == .testAgent && !model.agentTestEnabled)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.05)))
    }

    @ViewBuilder private func statusIcon(_ status: DoctorStatus) -> some View {
        switch status {
        case .ok: Image(systemName: "checkmark.circle.fill").foregroundStyle(WidgetPalette.ok).font(.system(size: 18))
        case .warning: Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(WidgetPalette.amber).font(.system(size: 17))
        case .problem: Image(systemName: "xmark.octagon.fill").foregroundStyle(WidgetPalette.rec).font(.system(size: 18))
        case .checking: ProgressView().controlSize(.small)
        }
    }
}

@MainActor
final class DoctorWindowController: NSWindowController, NSWindowDelegate {
    private let model: DoctorModel

    init(model: DoctorModel) {
        self.model = model
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 560),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.title = String(localized: "Shepit Doctor")
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.isOpaque = false
        window.backgroundColor = .clear
        window.appearance = NSAppearance(named: .darkAqua)
        let glass = NSVisualEffectView()
        glass.material = .hudWindow
        glass.blendingMode = .behindWindow
        glass.state = .active
        let hosting = NSHostingView(rootView: DoctorView(model: model))
        hosting.frame = glass.bounds
        hosting.autoresizingMask = [.width, .height]
        glass.addSubview(hosting)
        window.contentView = glass
        window.setContentSize(NSSize(width: 600, height: 560))
        window.center()
        super.init(window: window)
        window.delegate = self
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    func show() {
        model.start()
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        model.stop()
    }
}
