import Foundation

/// Where the text is going: the app and field that had focus when recording started.
public struct TargetContext: Codable, Sendable, Equatable {
    public var bundleID: String?
    public var appName: String?
    public var windowTitle: String?
    public var url: String?
    public var textBeforeCaret: String?
    public var selectedText: String?
    public var hasEditableFocus: Bool
    public var secureInput: Bool

    public init(bundleID: String? = nil, appName: String? = nil, windowTitle: String? = nil, url: String? = nil,
                textBeforeCaret: String? = nil, selectedText: String? = nil, hasEditableFocus: Bool = false,
                secureInput: Bool = false) {
        self.bundleID = bundleID
        self.appName = appName
        self.windowTitle = windowTitle
        self.url = url
        self.textBeforeCaret = textBeforeCaret
        self.selectedText = selectedText
        self.hasEditableFocus = hasEditableFocus
        self.secureInput = secureInput
    }
}
