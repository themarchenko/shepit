import Foundation

public enum HotkeyChoice: String, CaseIterable, Codable, Sendable {
    case rightOption
    case rightCommand
    case fn

    public var displayName: String {
        switch self {
        case .rightOption: return L("Right Option (⌥)")
        case .rightCommand: return L("Right Command (⌘)")
        case .fn: return L("Fn / Globe (needs \"Press Fn key to: Do Nothing\" in System Settings)")
        }
    }

    /// The key's name without the explanation, for menus and sentences.
    public var shortName: String {
        switch self {
        case .rightOption: return L("Right Option")
        case .rightCommand: return L("Right Command")
        case .fn: return L("Fn / Globe")
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
        case .off: return L("Off")
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
        case .flowPill: return L("Flow pill")
        case .captionBar: return L("Caption bar")
        case .island: return L("Island")
        }
    }

    public var blurb: String {
        switch self {
        case .flowPill: return L("A dash that springs into a pill. Words float above it.")
        case .captionBar: return L("A wide bar that reads like subtitles. Best for long prompts.")
        case .island: return L("One shape that grows into a card with the text and steps.")
        }
    }
}

/// The language of Shepit's own interface. The choice is stored as `AppleLanguages` in the app's defaults
/// domain, so macOS picks the localization at the next launch. The running language never changes.
public enum InterfaceLanguage: String, CaseIterable, Sendable {
    case system
    case english = "en"
    case ukrainian = "uk"

    /// Languages Shepit ships. Keep in step with CFBundleLocalizations in project.yml.
    public static let available = ["en", "uk"]

    static let defaultsKey = "AppleLanguages"

    /// The language macOS would run Shepit in: the first system language Shepit ships, else English.
    public static func target(system: [String]) -> String {
        for tag in system {
            let code = base(of: tag)
            if available.contains(code) { return code }
        }
        return "en"
    }

    /// A restart helps only if the language that would load differs from the one running now.
    public func needsRestart(running: String, system: [String]) -> Bool {
        let wanted = self == .system ? InterfaceLanguage.target(system: system) : rawValue
        return wanted != running
    }

    /// The user's system language list, read from the global domain (the app's own override is not in it).
    public static func systemLanguages() -> [String] {
        UserDefaults.standard.persistentDomain(forName: UserDefaults.globalDomain)?[defaultsKey] as? [String] ?? []
    }

    /// Language names are shown in their own language, whichever language the interface is in.
    public var displayName: String {
        switch self {
        case .system: return L("System")
        case .english: return InterfaceLanguage.endonym("en")
        case .ukrainian: return InterfaceLanguage.endonym("uk")
        }
    }

    static func base(of tag: String) -> String {
        String(tag.lowercased().prefix { $0 != "-" && $0 != "_" })
    }

    private static func endonym(_ code: String) -> String {
        let name = Locale(identifier: code).localizedString(forLanguageCode: code) ?? code
        return name.prefix(1).uppercased() + name.dropFirst()
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
    @Published public var reviewTipShown: Bool { didSet { defaults.set(reviewTipShown, forKey: Keys.reviewTipShown) } }
    /// Set when the voice calibration wizard has been opened on its own once; it never opens itself again.
    @Published public var calibrationOffered: Bool { didSet { defaults.set(calibrationOffered, forKey: Keys.calibrationOffered) } }
    /// The newest "What's new" version the user has seen.
    @Published public var whatsNewSeen: String { didSet { defaults.set(whatsNewSeen, forKey: Keys.whatsNewSeen) } }
    @Published public var claudeModel: ClaudeModel { didSet { defaults.set(claudeModel.rawValue, forKey: Keys.claudeModel) } }
    @Published public var claudeExecutable: String { didSet { defaults.set(claudeExecutable, forKey: Keys.claudeExecutable) } }
    @Published public var defaultMode: Mode { didSet { defaults.set(defaultMode.rawValue, forKey: Keys.defaultMode) } }
    @Published public var widgetVisible: Bool { didSet { defaults.set(widgetVisible, forKey: Keys.widgetVisible) } }
    @Published public var fastPathMaxWords: Int { didSet { defaults.set(fastPathMaxWords, forKey: Keys.fastPathMaxWords) } }
    @Published public var promptModeMinWords: Int { didSet { defaults.set(promptModeMinWords, forKey: Keys.promptModeMinWords) } }
    @Published public var language: LanguagePolicy { didSet { defaults.set(language.rawValue, forKey: Keys.languageLock) } }
    /// Show the words while speaking (Parakeet runs alongside the selected recognizer).
    @Published public var livePreview: Bool { didSet { defaults.set(livePreview, forKey: Keys.livePreview) } }
    /// Show the formatted text in an editable panel before it is pasted.
    @Published public var reviewBeforePaste: Bool { didSet { defaults.set(reviewBeforePaste, forKey: Keys.reviewBeforePaste) } }
    @Published public var dictionary: [String] { didSet { defaults.set(dictionary, forKey: Keys.dictionary) } }
    /// Free text about the speaker that goes into the formatter's system prompt: accent, typical recognizer confusions, habits.
    @Published public var speakerNotes: String { didSet { defaults.set(speakerNotes, forKey: Keys.speakerNotes) } }
    /// Ask GitHub about once a day whether a newer release is out.
    @Published public var checkForUpdates: Bool { didSet { defaults.set(checkForUpdates, forKey: Keys.checkForUpdates) } }
    /// A version the user chose to skip; scheduled checks stay quiet about it.
    @Published public var skippedUpdate: String { didSet { defaults.set(skippedUpdate, forKey: Keys.skippedUpdate) } }
    @Published public var lastUpdateCheck: Date { didSet { defaults.set(lastUpdateCheck, forKey: Keys.lastUpdateCheck) } }
    /// Applies at the next launch. Writing it never changes the language of the running app.
    @Published public var interfaceLanguage: InterfaceLanguage {
        didSet {
            if interfaceLanguage == .system { defaults.removeObject(forKey: InterfaceLanguage.defaultsKey) }
            else { defaults.set([interfaceLanguage.rawValue], forKey: InterfaceLanguage.defaultsKey) }
        }
    }

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
        static let reviewTipShown = "reviewTipShown"
        static let calibrationOffered = "calibrationOffered"
        static let whatsNewSeen = "whatsNewSeen"
        static let claudeModel = "claudeModel"
        static let claudeExecutable = "claudeExecutable"
        static let defaultMode = "defaultMode"
        static let widgetVisible = "widgetVisible"
        static let fastPathMaxWords = "fastPathMaxWords"
        static let promptModeMinWords = "promptModeMinWords"
        static let languageLock = "languageLock"
        static let livePreview = "livePreview"
        static let reviewBeforePaste = "reviewBeforePaste"
        static let dictionary = "dictionary"
        static let speakerNotes = "speakerNotes"
        static let widgetAnchor = "widgetAnchor"
        static let checkForUpdates = "checkForUpdates"
        static let skippedUpdate = "skippedUpdate"
        static let lastUpdateCheck = "lastUpdateCheck"
        /// Per-display positions keyed by display number, before 2026-10-26; removed on launch.
        static let legacyWidgetPositions = "widgetPositions"
    }

    /// `domain` is where the app-language override lives; only that domain is read, so the system's own
    /// language list never counts as a choice.
    public init(defaults: UserDefaults = .standard, domain: String? = Bundle.main.bundleIdentifier) {
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
        reviewTipShown = defaults.object(forKey: Keys.reviewTipShown) as? Bool ?? false
        calibrationOffered = defaults.object(forKey: Keys.calibrationOffered) as? Bool ?? false
        whatsNewSeen = defaults.string(forKey: Keys.whatsNewSeen) ?? ""
        claudeModel = ClaudeModel(rawValue: defaults.string(forKey: Keys.claudeModel) ?? "") ?? .haiku
        claudeExecutable = defaults.string(forKey: Keys.claudeExecutable) ?? ""
        defaultMode = Mode(rawValue: defaults.string(forKey: Keys.defaultMode) ?? "") ?? .clean
        widgetVisible = defaults.object(forKey: Keys.widgetVisible) as? Bool ?? true
        fastPathMaxWords = defaults.object(forKey: Keys.fastPathMaxWords) as? Int ?? 8
        promptModeMinWords = defaults.object(forKey: Keys.promptModeMinWords) as? Int ?? 40
        language = LanguagePolicy(stored: defaults.string(forKey: Keys.languageLock))
        livePreview = defaults.object(forKey: Keys.livePreview) as? Bool ?? true
        reviewBeforePaste = defaults.object(forKey: Keys.reviewBeforePaste) as? Bool ?? false
        dictionary = defaults.stringArray(forKey: Keys.dictionary) ?? []
        speakerNotes = defaults.string(forKey: Keys.speakerNotes) ?? SpeakerNotes.defaultText
        checkForUpdates = defaults.object(forKey: Keys.checkForUpdates) as? Bool ?? true
        skippedUpdate = defaults.string(forKey: Keys.skippedUpdate) ?? ""
        lastUpdateCheck = defaults.object(forKey: Keys.lastUpdateCheck) as? Date ?? .distantPast
        let stored = domain.flatMap { defaults.persistentDomain(forName: $0)?[InterfaceLanguage.defaultsKey] as? [String] }
        let first = stored?.first.map(InterfaceLanguage.base(of:))
        // A language Shepit does not ship counts as "System" and is left in place.
        interfaceLanguage = first.flatMap { InterfaceLanguage.available.contains($0) ? InterfaceLanguage(rawValue: $0) : nil } ?? .system
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

/// Text for the speaker section of the formatter prompt (editable in Settings).
public enum SpeakerNotes {
    /// Empty: the voice calibration writes notes from what the speaker actually sounds like.
    public static let defaultText = ""

    /// The default before 0.6.0. The prompt golden and Windows still use it; a stored copy counts as unset.
    public static let legacyDefault = """
    Native Ukrainian speaker who also dictates in English and mixes English technical terms into Ukrainian sentences.
    """

    public static func isUnset(_ notes: String) -> Bool {
        let trimmed = notes.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty || trimmed == legacyDefault.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
