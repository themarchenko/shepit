import Foundation

/// Clears a stale Accessibility grant for this app so macOS asks again. A grant made to an older build
/// can stay in the list switched on while the current build is refused.
public enum AccessibilityReset {
    static let tool = "/usr/bin/tccutil"

    /// Fixed argument list. Without a bundle id `tccutil reset` clears the permission of every app, so a
    /// missing or odd id yields nil and nothing runs.
    static func arguments(bundleID: String?) -> [String]? {
        guard let id = bundleID, !id.hasPrefix("-"),
              id.range(of: #"^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$"#, options: .regularExpression) != nil
        else { return nil }
        return ["reset", "Accessibility", id]
    }

    public static func perform(bundleID: String?) async -> Bool {
        await perform(bundleID: bundleID, run: UpdateInstaller.run)
    }

    static func perform(bundleID: String?, run: (String, [String]) async throws -> String) async -> Bool {
        guard let args = arguments(bundleID: bundleID) else { return false }
        do {
            _ = try await run(tool, args)
            return true
        } catch {
            Log.pipeline.error("accessibility reset failed: \(String(describing: type(of: error)), privacy: .public)")
            return false
        }
    }
}
