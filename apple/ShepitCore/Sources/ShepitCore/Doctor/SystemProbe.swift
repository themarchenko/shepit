import AppKit
import Carbon
import ApplicationServices
import Foundation

/// Thin readers of system state for Doctor. The parsing is split out so tests can cover it.
@MainActor
public enum SystemProbe {
    public static var accessibilityTrusted: Bool { ContextProbe.isAccessibilityTrusted }

    public static var listenEventAccess: Bool { CGPreflightListenEventAccess() }

    @discardableResult
    public static func requestListenEventAccess() -> Bool { CGRequestListenEventAccess() }

    /// "Press Fn key to" in Keyboard settings: 0 is Do Nothing. Nil when the value cannot be read.
    public static func fnUsageType() -> Int? {
        let domain = "com.apple.HIToolbox" as CFString
        CFPreferencesAppSynchronize(domain)
        return parseFnUsageType(CFPreferencesCopyAppValue("AppleFnUsageType" as CFString, domain))
    }

    nonisolated static func parseFnUsageType(_ value: Any?) -> Int? {
        (value as? NSNumber)?.intValue
    }

    /// Whether secure event input is on and the name of the app holding it, when macOS tells us.
    public static func secureInput() -> (active: Bool, holder: String?) {
        guard IsSecureEventInputEnabled() else { return (false, nil) }
        let session = CGSessionCopyCurrentDictionary() as? [String: Any]
        let name = secureInputPID(in: session).flatMap { NSRunningApplication(processIdentifier: $0)?.localizedName }
        return (true, name)
    }

    nonisolated static func secureInputPID(in session: [String: Any]?) -> pid_t? {
        guard let n = session?["kCGSSessionSecureInputPID"] as? Int, n > 0 else { return nil }
        return pid_t(n)
    }
}
