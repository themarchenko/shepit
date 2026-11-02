import Foundation

/// Clears a stale privacy grant for this app so macOS asks again. A grant made to an older build
/// can stay in the list switched on while the current build is refused.
public enum PermissionReset {
    /// The raw values are the TCC service names `tccutil` takes.
    public enum Service: String, Sendable { case accessibility = "Accessibility", microphone = "Microphone" }

    static let tool = "/usr/bin/tccutil"

    /// Fixed argument list. Without a bundle id `tccutil reset` clears the permission of every app, so a
    /// missing or odd id yields nil and nothing runs.
    static func arguments(service: Service, bundleID: String?) -> [String]? {
        guard let id = bundleID, !id.hasPrefix("-"),
              id.range(of: #"^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$"#, options: .regularExpression) != nil
        else { return nil }
        return ["reset", service.rawValue, id]
    }

    public static func perform(service: Service, bundleID: String?) async -> Bool {
        await perform(service: service, bundleID: bundleID, run: UpdateInstaller.run)
    }

    static func perform(service: Service, bundleID: String?, run: (String, [String]) async throws -> String) async -> Bool {
        guard let args = arguments(service: service, bundleID: bundleID) else { return false }
        do {
            _ = try await run(tool, args)
            return true
        } catch {
            Log.pipeline.error("\(service.rawValue, privacy: .public) reset failed: \(String(describing: type(of: error)), privacy: .public)")
            return false
        }
    }
}
