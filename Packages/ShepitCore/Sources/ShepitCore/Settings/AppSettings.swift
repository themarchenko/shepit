import Foundation

public enum HotkeyChoice: String, CaseIterable, Codable, Sendable {
    case rightOption
    case rightCommand
    case fn

    public var displayName: String {
        switch self {
        case .rightOption: return "Right Option (⌥)"
        case .rightCommand: return "Right Command (⌘)"
        case .fn: return "Fn / Globe (needs \"Press Fn key to: Do Nothing\" in System Settings)"
        }
    }

    /// Short label for key caps in the widget and the tour.
    public var keyLabel: String {
        switch self {
        case .rightOption: return "Right ⌥"
        case .rightCommand: return "Right ⌘"
        case .fn: return "Fn"
        }
    }
}

public enum ClaudeModel: String, CaseIterable, Codable, Sendable {
    case haiku
    case sonnet

    public var displayName: String { rawValue.capitalized }
}

/// Which agent polishes the recognized text. Off inserts it as heard (local cleanup and dictionary only).
public enum PolishingProvider: String, CaseIterable, Codable, Sendable {
    case claudeCode = "claude"
    case codex
    case off

    public var displayName: String {
        switch self {
        case .claudeCode: return "Claude Code"
        case .codex: return "Codex"
        case .off: return "Off"
        }
    }

    /// Executable name and the usual install locations, newest conventions first.
    public var executableName: String? {
        switch self {
        case .claudeCode: return "claude"
        case .codex: return "codex"
        case .off: return nil
        }
    }

    /// First executable found for this provider: the configured path, then common install locations.
    public func resolveExecutable(configured: String) -> String? {
        if !configured.isEmpty { return FileManager.default.isExecutableFile(atPath: configured) ? configured : nil }
        guard let name = executableName else { return nil }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let candidates = ["\(home)/.local/bin/\(name)", "/opt/homebrew/bin/\(name)", "/usr/local/bin/\(name)",
                          "\(home)/.npm-global/bin/\(name)", "\(home)/.bun/bin/\(name)"]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }
}

public enum WidgetStyle: String, CaseIterable, Codable, Sendable {
    case flowPill
    case captionBar
    case island

    public var displayName: String {
        switch self {
        case .flowPill: return "Flow pill"
        case .captionBar: return "Caption bar"
        case .island: return "Island"
        }
    }

    public var blurb: String {
        switch self {
        case .flowPill: return "A dash that springs into a pill. Words float above it."
        case .captionBar: return "A wide bar that reads like subtitles. Best for long prompts."
        case .island: return "One shape that grows into a card with the text and steps."
        }
    }
}

/// User preferences, backed by UserDefaults. All reads and writes happen on the main actor.
@MainActor
public final class AppSettings: ObservableObject {
    public static let shared = AppSettings()

    private let defaults: UserDefaults

    @Published public var engine: EngineID { didSet { defaults.set(engine.rawValue, forKey: Keys.engine) } }
    @Published public var hotkey: HotkeyChoice { didSet { defaults.set(hotkey.rawValue, forKey: Keys.hotkey) } }
    @Published public var polishing: PolishingProvider { didSet { defaults.set(polishing.rawValue, forKey: Keys.polishing) } }
    @Published public var codexExecutable: String { didSet { defaults.set(codexExecutable, forKey: Keys.codexExecutable) } }
    @Published public var widgetStyle: WidgetStyle { didSet { defaults.set(widgetStyle.rawValue, forKey: Keys.widgetStyle) } }
    /// Opacity of the widget's black glass, 0.5...0.9.
    @Published public var glassOpacity: Double { didSet { defaults.set(glassOpacity, forKey: Keys.glassOpacity) } }
    @Published public var fadeWhenIdle: Bool { didSet { defaults.set(fadeWhenIdle, forKey: Keys.fadeWhenIdle) } }
    @Published public var onboardingCompleted: Bool { didSet { defaults.set(onboardingCompleted, forKey: Keys.onboardingCompleted) } }
    /// The newest "What's new" version the user has seen.
    @Published public var whatsNewSeen: String { didSet { defaults.set(whatsNewSeen, forKey: Keys.whatsNewSeen) } }
    @Published public var claudeModel: ClaudeModel { didSet { defaults.set(claudeModel.rawValue, forKey: Keys.claudeModel) } }
    @Published public var claudeExecutable: String { didSet { defaults.set(claudeExecutable, forKey: Keys.claudeExecutable) } }
    @Published public var defaultMode: Mode { didSet { defaults.set(defaultMode.rawValue, forKey: Keys.defaultMode) } }
    @Published public var widgetVisible: Bool { didSet { defaults.set(widgetVisible, forKey: Keys.widgetVisible) } }
    @Published public var fastPathMaxWords: Int { didSet { defaults.set(fastPathMaxWords, forKey: Keys.fastPathMaxWords) } }
    @Published public var promptModeMinWords: Int { didSet { defaults.set(promptModeMinWords, forKey: Keys.promptModeMinWords) } }
    @Published public var languageLock: String { didSet { defaults.set(languageLock, forKey: Keys.languageLock) } }
    /// Show the words while speaking (Parakeet runs alongside the selected recognizer).
    @Published public var livePreview: Bool { didSet { defaults.set(livePreview, forKey: Keys.livePreview) } }
    @Published public var dictionary: [String] { didSet { defaults.set(dictionary, forKey: Keys.dictionary) } }
    /// Free text about the speaker that goes into the formatter's system prompt: accent, typical recognizer confusions, habits.
    @Published public var speakerNotes: String { didSet { defaults.set(speakerNotes, forKey: Keys.speakerNotes) } }
    /// Ask GitHub about once a day whether a newer release is out.
    @Published public var checkForUpdates: Bool { didSet { defaults.set(checkForUpdates, forKey: Keys.checkForUpdates) } }
    /// A version the user chose to skip; scheduled checks stay quiet about it.
    @Published public var skippedUpdate: String { didSet { defaults.set(skippedUpdate, forKey: Keys.skippedUpdate) } }
    @Published public var lastUpdateCheck: Date { didSet { defaults.set(lastUpdateCheck, forKey: Keys.lastUpdateCheck) } }

    private enum Keys {
        static let engine = "engine"
        static let hotkey = "hotkey"
        static let formatterEnabled = "formatterEnabled"
        static let polishing = "polishing"
        static let codexExecutable = "codexExecutable"
        static let widgetStyle = "widgetStyle"
        static let glassOpacity = "glassOpacity"
        static let fadeWhenIdle = "fadeWhenIdle"
        static let onboardingCompleted = "onboardingCompleted"
        static let whatsNewSeen = "whatsNewSeen"
        static let claudeModel = "claudeModel"
        static let claudeExecutable = "claudeExecutable"
        static let defaultMode = "defaultMode"
        static let widgetVisible = "widgetVisible"
        static let fastPathMaxWords = "fastPathMaxWords"
        static let promptModeMinWords = "promptModeMinWords"
        static let languageLock = "languageLock"
        static let livePreview = "livePreview"
        static let dictionary = "dictionary"
        static let speakerNotes = "speakerNotes"
        static let widgetAnchor = "widgetAnchor"
        static let checkForUpdates = "checkForUpdates"
        static let skippedUpdate = "skippedUpdate"
        static let lastUpdateCheck = "lastUpdateCheck"
        /// Per-display positions keyed by display number, before 2026-10-26; removed on launch.
        static let legacyWidgetPositions = "widgetPositions"
    }

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        engine = EngineID(rawValue: defaults.string(forKey: Keys.engine) ?? "") ?? .parakeetV3
        hotkey = HotkeyChoice(rawValue: defaults.string(forKey: Keys.hotkey) ?? "") ?? .rightOption
        // Before 0.2 polishing was a Claude on/off switch.
        let legacyOn = defaults.object(forKey: Keys.formatterEnabled) as? Bool ?? true
        polishing = PolishingProvider(rawValue: defaults.string(forKey: Keys.polishing) ?? "") ?? (legacyOn ? .claudeCode : .off)
        codexExecutable = defaults.string(forKey: Keys.codexExecutable) ?? ""
        widgetStyle = WidgetStyle(rawValue: defaults.string(forKey: Keys.widgetStyle) ?? "") ?? .flowPill
        glassOpacity = defaults.object(forKey: Keys.glassOpacity) as? Double ?? 0.7
        fadeWhenIdle = defaults.object(forKey: Keys.fadeWhenIdle) as? Bool ?? true
        onboardingCompleted = defaults.object(forKey: Keys.onboardingCompleted) as? Bool ?? false
        whatsNewSeen = defaults.string(forKey: Keys.whatsNewSeen) ?? ""
        claudeModel = ClaudeModel(rawValue: defaults.string(forKey: Keys.claudeModel) ?? "") ?? .haiku
        claudeExecutable = defaults.string(forKey: Keys.claudeExecutable) ?? ""
        defaultMode = Mode(rawValue: defaults.string(forKey: Keys.defaultMode) ?? "") ?? .clean
        widgetVisible = defaults.object(forKey: Keys.widgetVisible) as? Bool ?? true
        fastPathMaxWords = defaults.object(forKey: Keys.fastPathMaxWords) as? Int ?? 8
        promptModeMinWords = defaults.object(forKey: Keys.promptModeMinWords) as? Int ?? 40
        languageLock = defaults.string(forKey: Keys.languageLock) ?? "auto"
        livePreview = defaults.object(forKey: Keys.livePreview) as? Bool ?? true
        dictionary = defaults.stringArray(forKey: Keys.dictionary) ?? []
        speakerNotes = defaults.string(forKey: Keys.speakerNotes) ?? SpeakerNotes.defaultText
        checkForUpdates = defaults.object(forKey: Keys.checkForUpdates) as? Bool ?? true
        skippedUpdate = defaults.string(forKey: Keys.skippedUpdate) ?? ""
        lastUpdateCheck = defaults.object(forKey: Keys.lastUpdateCheck) as? Date ?? .distantPast
        defaults.removeObject(forKey: Keys.legacyWidgetPositions)
    }

    /// Where the widget sits: the middle of its bottom edge, in global screen coordinates.
    public var widgetAnchor: CGPoint? {
        get {
            guard let p = defaults.dictionary(forKey: Keys.widgetAnchor) as? [String: Double],
                  let x = p["x"], let y = p["y"] else { return nil }
            return CGPoint(x: x, y: y)
        }
        set {
            if let newValue { defaults.set(["x": newValue.x, "y": newValue.y], forKey: Keys.widgetAnchor) }
            else { defaults.removeObject(forKey: Keys.widgetAnchor) }
        }
    }

    /// Resolved path of the `claude` executable: the configured one, or the first common location that exists.
    public var resolvedClaudeExecutable: String? {
        PolishingProvider.claudeCode.resolveExecutable(configured: claudeExecutable)
    }

    public var resolvedCodexExecutable: String? {
        PolishingProvider.codex.resolveExecutable(configured: codexExecutable)
    }
}

public enum AppPaths {
    /// ~/Library/Application Support/Shepit
    public static var supportDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = base.appendingPathComponent("Shepit", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    public static var modelsDirectory: URL {
        let dir = supportDirectory.appendingPathComponent("models", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    public static var claudeWorkDirectory: URL {
        let dir = supportDirectory.appendingPathComponent("claude-work", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Working directory of the Codex app-server: empty, so Codex finds no project files or AGENTS.md there.
    public static var codexWorkDirectory: URL {
        let dir = supportDirectory.appendingPathComponent("codex-work", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    public static var databaseURL: URL {
        supportDirectory.appendingPathComponent("shepit.sqlite")
    }

    /// Audio of recent dictations, 16 kHz mono WAV, pruned by `RecordingStore`.
    public static var recordingsDirectory: URL {
        let dir = supportDirectory.appendingPathComponent("recordings", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}

/// Default text for the speaker section of the formatter prompt (editable in Settings).
public enum SpeakerNotes {
    public static let defaultText = """
    Native Ukrainian speaker who also dictates in English and mixes English technical terms into Ukrainian sentences.
    """
}
