import AppKit
import ApplicationServices
import Carbon
import Foundation

/// Reads the frontmost app and, when Accessibility is granted, the focused window and text field.
@MainActor
public enum ContextProbe {
    public static var isAccessibilityTrusted: Bool { AXIsProcessTrusted() }

    /// Shows the system prompt that sends the user to Privacy & Security > Accessibility.
    @discardableResult
    public static func requestAccessibility() -> Bool {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        return AXIsProcessTrustedWithOptions(options)
    }

    /// Whether the focused field's text (before the caret, selection) may be read. A password field must
    /// not leak into the agent prompt or the history; the app name and window title are still captured.
    nonisolated static func mayReadText(secureEventInput: Bool, subrole: String?) -> Bool {
        !secureEventInput && subrole != kAXSecureTextFieldSubrole
    }

    /// Secure event input right now, for a check after recording: focus may have moved into a password field.
    public static var isSecureInputActive: Bool { IsSecureEventInputEnabled() }

    public static func capture() -> TargetContext {
        var ctx = TargetContext()
        ctx.secureInput = IsSecureEventInputEnabled()
        guard let app = NSWorkspace.shared.frontmostApplication else { return ctx }
        ctx.bundleID = app.bundleIdentifier
        ctx.appName = app.localizedName
        guard AXIsProcessTrusted() else { return ctx }

        let axApp = AXUIElementCreateApplication(app.processIdentifier)
        if let window = element(axApp, kAXFocusedWindowAttribute) {
            ctx.windowTitle = string(window, kAXTitleAttribute)
        }
        if let focused = element(axApp, kAXFocusedUIElementAttribute) {
            let role = string(focused, kAXRoleAttribute) ?? ""
            let editableRoles: Set<String> = [kAXTextFieldRole, kAXTextAreaRole, kAXComboBoxRole, "AXSearchField"]
            let subrole = string(focused, kAXSubroleAttribute)
            ctx.hasEditableFocus = editableRoles.contains(role) || subrole == kAXSecureTextFieldSubrole
            if subrole == kAXSecureTextFieldSubrole { ctx.secureInput = true }
            guard mayReadText(secureEventInput: IsSecureEventInputEnabled(), subrole: subrole) else { return ctx }
            if let value = string(focused, kAXValueAttribute), value.count < 50_000 {
                if let range = range(focused, kAXSelectedTextRangeAttribute) {
                    let location = min(max(0, range.location), value.count)
                    ctx.textBeforeCaret = String(value.prefix(location).suffix(500))
                } else {
                    ctx.textBeforeCaret = String(value.suffix(500))
                }
            }
            if let selected = string(focused, kAXSelectedTextAttribute), !selected.isEmpty, selected.count < 10_000 {
                ctx.selectedText = selected
            }
        }
        return ctx
    }

    // MARK: AX helpers

    private static func value(_ element: AXUIElement, _ attribute: String) -> CFTypeRef? {
        var out: CFTypeRef?
        let err = AXUIElementCopyAttributeValue(element, attribute as CFString, &out)
        return err == .success ? out : nil
    }

    private static func element(_ element: AXUIElement, _ attribute: String) -> AXUIElement? {
        guard let v = value(element, attribute), CFGetTypeID(v) == AXUIElementGetTypeID() else { return nil }
        return (v as! AXUIElement)
    }

    private static func string(_ element: AXUIElement, _ attribute: String) -> String? {
        value(element, attribute) as? String
    }

    private static func range(_ element: AXUIElement, _ attribute: String) -> CFRange? {
        guard let v = value(element, attribute), CFGetTypeID(v) == AXValueGetTypeID() else { return nil }
        var r = CFRange()
        return AXValueGetValue(v as! AXValue, .cfRange, &r) ? r : nil
    }
}
