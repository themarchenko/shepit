import AppKit
import Combine
import ServiceManagement
import SwiftUI
import ShepitCore

/// What the Settings window can ask the rest of the app to do.
struct SettingsActions {
    var resetWidgetPosition: () -> Void = {}
    var showWhatsNew: () -> Void = {}
    var showWelcomeTour: () -> Void = {}
    var showUpdate: () -> Void = {}
}

struct GeneralPane: View {
    @ObservedObject var settings: AppSettings
    @State private var launchAtLogin = SMAppService.mainApp.status == .enabled
    @State private var accessibilityTrusted = ContextProbe.isAccessibilityTrusted
    @State private var microphoneGranted = AudioRecorder.permissionGranted

    var body: some View {
        Form {
            Section {
                Picker("Dictation key", selection: $settings.hotkey) {
                    ForEach(HotkeyChoice.allCases, id: \.self) { Text($0.displayName).tag($0) }
                }
                Text("Hold to talk, double tap for hands-free.").font(.caption).foregroundStyle(.secondary)
                Picker("Language", selection: $settings.languageLock) {
                    Text("Auto").tag("auto")
                    Text("Українська").tag("uk")
                    Text("English").tag("en")
                }
                .pickerStyle(.segmented)
                Picker("Default mode", selection: $settings.defaultMode) {
                    ForEach(Mode.allCases, id: \.self) { Text($0.displayName).tag($0) }
                }
                .pickerStyle(.segmented)
                Stepper("Prompt mode in AI apps from \(settings.promptModeMinWords) words",
                        value: $settings.promptModeMinWords, in: 10...200, step: 5)
            }
            Section("Recognition") {
                Picker("Recognizer", selection: $settings.engine) {
                    ForEach(EngineID.allCases, id: \.self) { Text($0.displayName).tag($0) }
                }
                Text("Runs on this Mac. Models download on first use (0.6-1.6 GB).").font(.caption).foregroundStyle(.secondary)
            }
            Section("System") {
                Toggle("Launch at login", isOn: $launchAtLogin)
                    .onChange(of: launchAtLogin) { _, on in
                        do {
                            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
                        } catch {
                            Log.ui.error("launch at login failed: \(String(describing: error), privacy: .public)")
                            launchAtLogin = SMAppService.mainApp.status == .enabled
                        }
                    }
                Toggle("Show floating widget", isOn: $settings.widgetVisible)
            }
            Section("Permissions") {
                permission("Microphone", granted: microphoneGranted) { openPrivacy("Privacy_Microphone") }
                permission("Accessibility (needed to paste)", granted: accessibilityTrusted, open: openAccessibilitySettings)
                if !accessibilityTrusted {
                    Text(accessibilityStaleHint).font(.caption).foregroundStyle(.secondary)
                }
                Button("Check again") {
                    accessibilityTrusted = ContextProbe.isAccessibilityTrusted
                    microphoneGranted = AudioRecorder.permissionGranted
                }
            }
        }
        .settingsForm()
    }

    private func permission(_ name: String, granted: Bool, open: @escaping () -> Void) -> some View {
        HStack {
            Label(name, systemImage: granted ? "checkmark.circle.fill" : "xmark.circle")
                .foregroundStyle(granted ? WidgetPalette.ok : WidgetPalette.amber)
            Spacer()
            if !granted { Button("Open Privacy settings", action: open) }
        }
    }
}

struct AppearancePane: View {
    @ObservedObject var settings: AppSettings
    var actions: SettingsActions

    var body: some View {
        Form {
            Section("Widget style") {
                StylePicker(settings: settings)
                    .padding(.vertical, 4)
            }
            Section {
                Toggle(isOn: $settings.livePreview) {
                    Text("Show the words while speaking")
                    Text("Runs Parakeet next to the recognizer")
                }
                LabeledContent("Glass") {
                    Slider(value: $settings.glassOpacity, in: 0.5...0.9) {
                        EmptyView()
                    } minimumValueLabel: {
                        Text("Lighter").font(.caption)
                    } maximumValueLabel: {
                        Text("Darker").font(.caption)
                    }
                    .frame(width: 220)
                }
                Toggle(isOn: $settings.fadeWhenIdle) {
                    Text("Fade when idle")
                    Text("Down to half after 3 seconds")
                }
                LabeledContent {
                    Button("Reset position", action: actions.resetWidgetPosition)
                } label: {
                    Text("Position")
                    Text("Drag the widget anywhere; it stays there across launches")
                }
            }
        }
        .settingsForm()
    }
}

struct PolishingPane: View {
    @ObservedObject var settings: AppSettings
    @State private var speakerNotesText: String

    init(settings: AppSettings) {
        self.settings = settings
        _speakerNotesText = State(initialValue: settings.speakerNotes)
    }

    var body: some View {
        Form {
            Section {
                ProviderList(settings: settings)
                    .padding(.vertical, 4)
            } header: {
                Text("Polishing")
            } footer: {
                Text("Recognition stays on this Mac. Polishing sends the recognized text, never the audio, to the agent you pick.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if settings.polishing == .claudeCode {
                Section("Claude Code") {
                    Picker("Model", selection: $settings.claudeModel) {
                        ForEach(ClaudeModel.allCases, id: \.self) { Text($0.displayName).tag($0) }
                    }
                    TextField("Executable", text: $settings.claudeExecutable, prompt: Text("Found automatically"))
                }
            }
            if settings.polishing == .codex {
                Section("Codex") {
                    TextField("Executable", text: $settings.codexExecutable, prompt: Text("Found automatically"))
                }
            }
            Section {
                Stepper("Skip polishing for phrases up to \(settings.fastPathMaxWords) words",
                        value: $settings.fastPathMaxWords, in: 0...30)
                Text("Outside AI apps. Dictionary terms are still fixed.").font(.caption).foregroundStyle(.secondary)
            }
            Section("About the speaker") {
                Text("Goes into the agent's instructions: accent, sounds the recognizer gets wrong, habits. Any language.")
                    .font(.caption).foregroundStyle(.secondary)
                TextEditor(text: $speakerNotesText)
                    .frame(minHeight: 90)
                    .onChange(of: speakerNotesText) { _, new in settings.speakerNotes = new }
            }
        }
        .settingsForm()
    }
}

struct DictionaryPane: View {
    @ObservedObject var settings: AppSettings
    @State private var dictionaryText: String

    init(settings: AppSettings) {
        self.settings = settings
        _dictionaryText = State(initialValue: settings.dictionary.joined(separator: "\n"))
    }

    private static func terms(in text: String) -> [String] {
        text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }

    var body: some View {
        Form {
            Section {
                TextEditor(text: $dictionaryText)
                    .font(.system(.body, design: .monospaced))
                    .frame(minHeight: 300)
                    .onChange(of: dictionaryText) { _, new in
                        settings.dictionary = Self.terms(in: new)
                    }
                    // Learned terms arrive while the window is open; the text is replaced only when the
                    // term lists differ, so typing a new line is not undone.
                    .onReceive(settings.$dictionary) { terms in
                        if Self.terms(in: dictionaryText) != terms { dictionaryText = terms.joined(separator: "\n") }
                    }
            } header: {
                Text("Terms")
            } footer: {
                Text("One per line: names, products and identifiers the recognizer gets wrong. Terms the polishing agent fixes are added here; the newest 30 also go to Whisper as a hint.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .settingsForm()
    }
}

struct AboutPane: View {
    @ObservedObject var settings: AppSettings
    @ObservedObject var updates: UpdateChecker
    var actions: SettingsActions

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    }

    var body: some View {
        Form {
            Section {
                HStack(spacing: 16) {
                    Image(nsImage: NSApp.applicationIconImage).resizable().frame(width: 64, height: 64)
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Shepit \(version)").font(.system(size: 16, weight: .semibold))
                        Text("Recognition runs on this Mac with Whisper and Parakeet. Polishing uses the agent you pick.")
                            .font(.system(size: 12)).foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
            }
            Section("Updates") {
                Toggle("Check for updates once a day", isOn: $settings.checkForUpdates)
                LabeledContent {
                    switch updates.state {
                    case .available: Button("Install…", action: actions.showUpdate)
                    case .checking, .downloading, .installing: ProgressView().controlSize(.small)
                    default: Button("Check now") { Task { await updates.check(userInitiated: true) } }
                    }
                } label: {
                    Text(updateStatus)
                    Text(updateDetail)
                }
            }
            Section {
                LabeledContent("What's new in \(version)") { Button("Show", action: actions.showWhatsNew) }
                LabeledContent("Welcome tour") { Button("Show again", action: actions.showWelcomeTour) }
                LabeledContent {
                    Button("Open folder") { NSWorkspace.shared.open(AppPaths.recordingsDirectory) }
                } label: {
                    Text("Recordings")
                    Text("Last 200 dictations, kept for comparing recognizers")
                }
            }
        }
        .settingsForm()
    }

    private var updateStatus: String {
        switch updates.state {
        case .checking: return "Checking GitHub…"
        case .available(let release): return "Shepit \(release.version) is out"
        case .downloading(let release, let fraction): return "Downloading \(release.version): \(Int(fraction * 100))%"
        case .installing(let release): return "Installing \(release.version)…"
        case .failed: return "Last check failed"
        case .idle, .upToDate: return "Up to date"
        }
    }

    private var updateDetail: String {
        if case .failed(let message, _) = updates.state { return message }
        guard settings.lastUpdateCheck > .distantPast else { return "Not checked yet" }
        return "Checked " + settings.lastUpdateCheck.formatted(.relative(presentation: .named))
    }
}

func openPrivacy(_ pane: String) {
    if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") {
        NSWorkspace.shared.open(url)
    }
}

/// macOS keeps the Accessibility grant of the previous signature, so the switch can look on while
/// the permission is not valid for this build. Removing the row is the only way to clear it.
let accessibilityStaleHint = "Already on in the list? Select Shepit, remove it with the minus button, then turn it on again. macOS keeps the permission of the previous build."

/// Asks for Accessibility first so Shepit is already in the list, then opens the list. Opening it
/// without asking leaves Shepit out, and the user has to add it with the + button.
@MainActor
func openAccessibilitySettings() {
    ContextProbe.requestAccessibility()
    openPrivacy("Privacy_Accessibility")
}

extension View {
    /// A grouped form on the window's black glass instead of the system background.
    func settingsForm() -> some View {
        formStyle(.grouped).scrollContentBackground(.hidden)
    }
}

/// The pane the Settings window shows. The controller resizes the window when it changes.
@MainActor
final class SettingsNavigation: ObservableObject {
    @Published var tab: SettingsWindowController.Tab = .appearance
}

/// Settings in the widget's black glass: a tab bar of symbols over the selected pane.
struct SettingsRoot: View {
    static let headerHeight: CGFloat = 78

    @ObservedObject var nav: SettingsNavigation
    @ObservedObject var settings: AppSettings
    var updates: UpdateChecker
    var actions: SettingsActions
    @Namespace private var selection

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 4) {
                ForEach(SettingsWindowController.Tab.allCases, id: \.self) { tabButton($0) }
            }
            .frame(maxWidth: .infinity)
            .frame(height: Self.headerHeight - 0.5)
            .background(Color.clear.contentShape(Rectangle()).gesture(WindowDragGesture()))
            Rectangle().fill(Color.white.opacity(0.08)).frame(height: 0.5)
            pane
                .id(nav.tab)
                .transition(.opacity)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .background(Color.black.opacity(0.55))
        .ignoresSafeArea()
        .environment(\.colorScheme, .dark)
    }

    @ViewBuilder private var pane: some View {
        switch nav.tab {
        case .general: GeneralPane(settings: settings)
        case .appearance: AppearancePane(settings: settings, actions: actions)
        case .polishing: PolishingPane(settings: settings)
        case .dictionary: DictionaryPane(settings: settings)
        case .about: AboutPane(settings: settings, updates: updates, actions: actions)
        }
    }

    private func tabButton(_ tab: SettingsWindowController.Tab) -> some View {
        let selected = nav.tab == tab
        return Button {
            withAnimation(.easeOut(duration: 0.2)) { nav.tab = tab }
        } label: {
            VStack(spacing: 4) {
                Image(systemName: tab.symbol).font(.system(size: 17)).frame(height: 20)
                Text(tab.title).font(.system(size: 11, weight: selected ? .semibold : .regular))
            }
            .foregroundStyle(WidgetPalette.text.opacity(selected ? 1 : 0.55))
            .frame(width: 80, height: 50)
            .background {
                if selected {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(Color.white.opacity(0.1))
                        .matchedGeometryEffect(id: "tab", in: selection)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(tab.title)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

@MainActor
final class SettingsWindowController: NSWindowController {
    enum Tab: Int, CaseIterable {
        case general, appearance, polishing, dictionary, about

        var title: String {
            switch self {
            case .general: return "General"
            case .appearance: return "Appearance"
            case .polishing: return "Polishing"
            case .dictionary: return "Dictionary"
            case .about: return "About"
            }
        }

        var symbol: String {
            switch self {
            case .general: return "gearshape"
            case .appearance: return "paintpalette"
            case .polishing: return "sparkles"
            case .dictionary: return "character.book.closed"
            case .about: return "info.circle"
            }
        }

        var paneHeight: CGFloat {
            switch self {
            case .general: return 600
            case .appearance: return 540
            case .polishing: return 660
            case .dictionary: return 460
            case .about: return 470
            }
        }
    }

    private static let width: CGFloat = 620
    private let nav = SettingsNavigation()
    private var tabChange: AnyCancellable?

    init(settings: AppSettings, updates: UpdateChecker, actions: SettingsActions) {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: Self.width, height: 600),
                              styleMask: [.titled, .closable, .fullSizeContentView], backing: .buffered, defer: false)
        window.title = "Settings"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.isOpaque = false
        window.backgroundColor = .clear
        window.appearance = NSAppearance(named: .darkAqua)

        // Blur what is behind the window; SettingsRoot darkens it to the widget's black glass.
        let glass = NSVisualEffectView()
        glass.material = .hudWindow
        glass.blendingMode = .behindWindow
        glass.state = .active
        let hosting = NSHostingView(rootView: SettingsRoot(nav: nav, settings: settings, updates: updates, actions: actions))
        hosting.sizingOptions = []
        hosting.frame = glass.bounds
        hosting.autoresizingMask = [.width, .height]
        glass.addSubview(hosting)
        window.contentView = glass
        super.init(window: window)

        resize(to: nav.tab, animate: false)
        window.center()
        tabChange = nav.$tab.dropFirst().removeDuplicates().sink { [weak self] tab in self?.resize(to: tab, animate: true) }
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Fits the window to the pane, keeping its top edge where it is.
    private func resize(to tab: Tab, animate: Bool) {
        guard let window else { return }
        let size = NSSize(width: Self.width, height: SettingsRoot.headerHeight + tab.paneHeight)
        var frame = window.frame
        frame.origin.y += frame.height - size.height
        frame.size = size
        window.setFrame(frame, display: true, animate: animate)
    }

    func show(tab: Tab? = nil) {
        if let tab { nav.tab = tab }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}
