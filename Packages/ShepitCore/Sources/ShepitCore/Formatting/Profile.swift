import Foundation

public enum Mode: String, CaseIterable, Codable, Sendable {
    case clean
    case prompt
    case raw

    public var displayName: String {
        switch self {
        case .clean: return "Clean"
        case .prompt: return "Prompt"
        case .raw: return "Raw"
        }
    }
}

/// How text should look for one kind of target app.
public struct Profile: Codable, Sendable, Equatable {
    public let id: String
    public let name: String
    /// The target is an AI assistant: long dictations are shaped as prompts.
    public let isAITarget: Bool
    public let allowsMarkdown: Bool
    public let styleRules: [String]
    public let stripTrailingNewline: Bool

    public init(id: String, name: String, isAITarget: Bool, allowsMarkdown: Bool, styleRules: [String], stripTrailingNewline: Bool = true) {
        self.id = id
        self.name = name
        self.isAITarget = isAITarget
        self.allowsMarkdown = allowsMarkdown
        self.styleRules = styleRules
        self.stripTrailingNewline = stripTrailingNewline
    }
}

public enum BuiltInProfiles {
    public static let claudeCodeTerminal = Profile(
        id: "claude-code-terminal", name: "Claude Code in a terminal", isAITarget: true, allowsMarkdown: false,
        styleRules: [
            "Plain text only: no markdown headers, no bold, no code fences; identifiers and paths stay bare.",
            "Lists use \"-\" bullets.",
            "No trailing newline. Never end with a question addressed to the user.",
        ])

    public static let terminal = Profile(
        id: "terminal", name: "Terminal", isAITarget: false, allowsMarkdown: false,
        styleRules: [
            "Plain text, single line unless the speaker clearly dictates several lines.",
            "Commands, flags and paths exactly as spoken, in Latin script.",
        ])

    public static let aiChat = Profile(
        id: "ai-chat", name: "AI chat app or site", isAITarget: true, allowsMarkdown: true,
        styleRules: [
            "Markdown lists are allowed; no headers for short prompts.",
            "Short paragraphs. Code identifiers in backticks.",
        ])

    public static let codeEditor = Profile(
        id: "code-editor", name: "Code editor", isAITarget: true, allowsMarkdown: true,
        styleRules: [
            "Assume the text goes into an AI chat panel or a comment: markdown allowed, identifiers in backticks.",
            "Keep camelCase and snake_case exactly as spoken.",
        ])

    public static let messenger = Profile(
        id: "messenger", name: "Messenger", isAITarget: false, allowsMarkdown: false,
        styleRules: [
            "Casual register, short sentences, no period at the end of a one-line message.",
            "Emoji only when the speaker names one.",
        ])

    public static let mail = Profile(
        id: "mail", name: "Email", isAITarget: false, allowsMarkdown: false,
        styleRules: [
            "Paragraphs separated by blank lines. Greeting and sign-off only when dictated.",
        ])

    public static let notes = Profile(
        id: "notes", name: "Notes and documents", isAITarget: false, allowsMarkdown: true,
        styleRules: [
            "Lists when the speaker enumerates, paragraphs otherwise.",
        ])

    public static let generic = Profile(
        id: "generic", name: "Other apps", isAITarget: false, allowsMarkdown: false,
        styleRules: [
            "Neutral register, plain text, paragraphs separated by blank lines.",
        ])

    public static let all: [Profile] = [claudeCodeTerminal, terminal, aiChat, codeEditor, messenger, mail, notes, generic]

    public static func byID(_ id: String) -> Profile? { all.first { $0.id == id } }
}

public enum ProfileResolver {
    static let terminals: Set<String> = [
        "com.apple.Terminal", "com.googlecode.iterm2", "com.mitchellh.ghostty", "dev.warp.Warp-Stable",
        "net.kovidgoyal.kitty", "com.github.wez.wezterm", "org.alacritty",
    ]
    static let aiApps: Set<String> = ["com.anthropic.claudefordesktop", "com.openai.chat", "com.openai.codex"]
    static let aiHosts: [String] = ["claude.ai", "chatgpt.com", "chat.openai.com", "gemini.google.com", "perplexity.ai", "copilot.microsoft.com"]
    static let codeEditors: Set<String> = ["com.microsoft.VSCode", "com.todesktop.230313mzl4w4u92", "com.apple.dt.Xcode", "dev.zed.Zed"]
    static let messengers: Set<String> = [
        "ru.keepcoder.Telegram", "com.tdesktop.Telegram", "com.tinyspeck.slackmacgap", "com.apple.MobileSMS",
        "com.hnc.Discord", "net.whatsapp.WhatsApp", "com.facebook.archon",
    ]
    static let mailApps: Set<String> = ["com.apple.mail", "com.readdle.smartemail-Mac", "com.superhuman.electron"]
    static let noteApps: Set<String> = ["com.apple.Notes", "md.obsidian", "notion.id", "com.apple.TextEdit", "com.apple.iWork.Pages", "com.bear-writer.bear"]
    static let mailHosts: [String] = ["mail.google.com", "outlook.live.com", "outlook.office.com"]
    static let docHosts: [String] = ["docs.google.com", "notion.so"]

    public static func resolve(_ ctx: TargetContext) -> Profile {
        let bundle = ctx.bundleID ?? ""
        let title = (ctx.windowTitle ?? "").lowercased()
        if terminals.contains(bundle) {
            return title.contains("claude") ? BuiltInProfiles.claudeCodeTerminal : BuiltInProfiles.terminal
        }
        if aiApps.contains(bundle) { return BuiltInProfiles.aiChat }
        if codeEditors.contains(bundle) { return BuiltInProfiles.codeEditor }
        if messengers.contains(bundle) { return BuiltInProfiles.messenger }
        if mailApps.contains(bundle) { return BuiltInProfiles.mail }
        if noteApps.contains(bundle) { return BuiltInProfiles.notes }
        if let host = hostOf(ctx.url) {
            if aiHosts.contains(where: { host == $0 || host.hasSuffix("." + $0) }) { return BuiltInProfiles.aiChat }
            if mailHosts.contains(where: { host == $0 }) { return BuiltInProfiles.mail }
            if docHosts.contains(where: { host == $0 || host.hasSuffix("." + $0) }) { return BuiltInProfiles.notes }
        }
        return BuiltInProfiles.generic
    }

    static func hostOf(_ url: String?) -> String? {
        guard let url, let host = URL(string: url)?.host?.lowercased() else { return nil }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }
}

public enum ModeResolver {
    /// Clean stays clean unless the target is an AI tool and the dictation is long enough to deserve structure.
    public static func resolve(requested: Mode, profile: Profile, wordCount: Int, promptModeMinWords: Int) -> Mode {
        if requested == .clean, profile.isAITarget, wordCount >= promptModeMinWords { return .prompt }
        return requested
    }

    public static func wordCount(_ text: String) -> Int {
        text.split(whereSeparator: { $0.isWhitespace || $0.isNewline }).count
    }
}
