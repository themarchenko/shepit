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
    /// Clipboard managers that follow nspasteboard.org skip items that carry these types.
    static let transientType = NSPasteboard.PasteboardType("org.nspasteboard.TransientType")
    static let concealedType = NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType")

    public var restoreDelay: Duration = .milliseconds(350)
    private let pasteboard: NSPasteboard
    // Seams for tests: the real values need Accessibility and a focused app.
    var isTrusted: () -> Bool = { AXIsProcessTrusted() }
    var sendPaste: () -> Void = { PasteInserter.postCommandV() }

    public init() {
        pasteboard = .general
    }

    init(pasteboard: NSPasteboard) {
        self.pasteboard = pasteboard
    }

    /// Puts the dictated text on the pasteboard, marked so clipboard managers do not keep it. Only this write
    /// is marked; the restore of the user's own contents is not, because that is their data to keep.
    static func writeDictation(_ text: String, to pasteboard: NSPasteboard) {
        pasteboard.clearContents()
        pasteboard.declareTypes([.string, transientType, concealedType], owner: nil)
        pasteboard.setString(text, forType: .string)
        pasteboard.setData(Data(), forType: transientType)
        pasteboard.setData(Data(), forType: concealedType)
    }

    public func insert(_ text: String, context: TargetContext) async throws -> InsertMethod {
        guard !text.isEmpty else { throw InsertError.empty }
        if context.secureInput || IsSecureEventInputEnabled() { throw InsertError.secureInput }

        let snapshot = PasteboardSnapshot(pasteboard)
        Self.writeDictation(text, to: pasteboard)
        let ourChangeCount = pasteboard.changeCount

        guard isTrusted() else {
            Log.insert.warning("accessibility not granted; text left on the clipboard")
            return .clipboardOnly
        }
        sendPaste()
        // A cancelled dictation must still give the clipboard back, but not before the delay: ⌘V is already
        // on its way, and restoring now could paste the user's own clipboard into the target instead.
        do {
            try await Task.sleep(for: restoreDelay)
        } catch {
            let delay = restoreDelay
            Task { @MainActor in
                try? await Task.sleep(for: delay)
                self.restoreIfUntouched(snapshot, ourChangeCount)
            }
            throw error
        }
        restoreIfUntouched(snapshot, ourChangeCount)
        return .paste
    }

    private func restoreIfUntouched(_ snapshot: PasteboardSnapshot, _ changeCount: Int) {
        if pasteboard.changeCount == changeCount {
            snapshot.restore(to: pasteboard)
        }
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
