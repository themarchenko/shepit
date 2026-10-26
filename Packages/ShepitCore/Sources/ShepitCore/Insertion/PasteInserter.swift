import AppKit
import ApplicationServices
import Carbon
import Foundation

public enum InsertMethod: String, Codable, Sendable {
    case accessibility
    case paste
    case typing
    case clipboardOnly
}

public enum InsertError: Error, LocalizedError {
    case secureInput
    case empty

    public var errorDescription: String? {
        switch self {
        case .secureInput: return "A password field is active; nothing was inserted"
        case .empty: return "Nothing to insert"
        }
    }
}

/// Copies the pasteboard so it can be put back after a paste.
struct PasteboardSnapshot {
    private let items: [NSPasteboardItem]

    init(_ pasteboard: NSPasteboard) {
        items = (pasteboard.pasteboardItems ?? []).compactMap { item in
            let copy = NSPasteboardItem()
            var any = false
            for type in item.types {
                if let data = item.data(forType: type) {
                    copy.setData(data, forType: type)
                    any = true
                }
            }
            return any ? copy : nil
        }
    }

    func restore(to pasteboard: NSPasteboard) {
        pasteboard.clearContents()
        if !items.isEmpty { pasteboard.writeObjects(items) }
    }
}

/// Inserts text by putting it on the pasteboard and sending ⌘V to the frontmost app, then restores the pasteboard.
@MainActor
public final class PasteInserter {
    public var restoreDelay: Duration = .milliseconds(350)

    public init() {}

    public func insert(_ text: String, context: TargetContext) async throws -> InsertMethod {
        guard !text.isEmpty else { throw InsertError.empty }
        if context.secureInput || IsSecureEventInputEnabled() { throw InsertError.secureInput }

        let pasteboard = NSPasteboard.general
        let snapshot = PasteboardSnapshot(pasteboard)
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        let ourChangeCount = pasteboard.changeCount

        guard AXIsProcessTrusted() else {
            Log.insert.warning("accessibility not granted; text left on the clipboard")
            return .clipboardOnly
        }
        Self.postCommandV()
        try await Task.sleep(for: restoreDelay)
        if pasteboard.changeCount == ourChangeCount {
            snapshot.restore(to: pasteboard)
        }
        return .paste
    }

    static func postCommandV() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let vKey: CGKeyCode = 9
        guard let down = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: true),
              let up = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: false) else { return }
        down.flags = .maskCommand
        up.flags = .maskCommand
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
    }
}
