import AppKit
import Foundation
import Security

/// Installs a release over the running app and relaunches it.
///
/// Quarantine is not the problem it is for a browser download: only apps that opt in mark what they
/// download, and Shepit does not. The flag is removed anyway in case the disk image carried one. The
/// check that matters is the code signature: the new app has to satisfy the running app's designated
/// requirement. Every release is signed with the same certificate, so a release passes and anything
/// else fails, including a release offered to a local build signed with another identity.
public enum UpdateInstaller {
    public enum Phase: Equatable, Sendable {
        case downloading(Double)
        case installing
    }

    public enum InstallError: LocalizedError {
        case translocated
        case notAnUpdate(String)
        case signatureMismatch
        case cancelled
        case tool(String, String)

        public var errorDescription: String? {
            switch self {
            case .translocated:
                return "Shepit is running from a temporary copy macOS made for it. Move Shepit to Applications, open it from there and update again."
            case .notAnUpdate(let detail):
                return "The download is not a newer Shepit (\(detail))."
            case .signatureMismatch:
                return "This copy of Shepit is signed differently from the releases, so it cannot update itself. Download the release from GitHub instead."
            case .cancelled:
                return "The update was cancelled."
            case .tool(let name, let output):
                return "\(name) failed: \(output)"
            }
        }
    }

    /// Downloads `release`, checks it against `target` (the running app unless a test says otherwise) and
    /// swaps it in. Returns the installed app, ready for `relaunch`.
    public static func install(_ release: AppRelease, over target: URL = Bundle.main.bundleURL,
                               progress: @escaping @Sendable (Phase) -> Void) async throws -> URL {
        guard !target.path.contains("/AppTranslocation/") else { throw InstallError.translocated }

        let work = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-update-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: work) }

        let image = work.appendingPathComponent("Shepit.dmg")
        try await Downloader.download(release.diskImageURL, to: image, expectedSize: release.diskImageSize) {
            progress(.downloading($0))
        }
        progress(.installing)

        let volume = work.appendingPathComponent("volume")
        let staged = work.appendingPathComponent("Shepit.app")
        try await run("/usr/bin/hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", volume.path, image.path])
        let copied = await Result { try await run("/usr/bin/ditto", [volume.appendingPathComponent("Shepit.app").path, staged.path]) }
        _ = try? await run("/usr/bin/hdiutil", ["detach", "-quiet", "-force", volume.path])
        _ = try copied.get()
        _ = try? await run("/usr/bin/xattr", ["-dr", "com.apple.quarantine", staged.path])

        try check(staged, release: release, replacing: target)
        try await swap(staged, into: target)
        Log.update.info("installed \(release.version, privacy: .public) at \(target.path, privacy: .public)")
        return target
    }

    /// Quits, and opens `app` again once this process is gone.
    @MainActor
    public static func relaunch(_ app: URL) {
        let pid = ProcessInfo.processInfo.processIdentifier
        let shell = Process()
        shell.executableURL = URL(fileURLWithPath: "/bin/sh")
        shell.arguments = ["-c", "while /bin/kill -0 \(pid) 2>/dev/null; do /bin/sleep 0.2; done; /usr/bin/open \"$0\"",
                           app.path]
        try? shell.run()
        NSApp.terminate(nil)
    }

    static func check(_ app: URL, release: AppRelease, replacing target: URL) throws {
        // Read the plists directly: Bundle caches by path, and a test reuses paths.
        func info(_ bundle: URL) -> [String: Any] {
            NSDictionary(contentsOf: bundle.appendingPathComponent("Contents/Info.plist")) as? [String: Any] ?? [:]
        }
        let new = info(app), old = info(target)
        let identifier = new["CFBundleIdentifier"] as? String
        let version = new["CFBundleShortVersionString"] as? String ?? "no version"
        guard identifier == old["CFBundleIdentifier"] as? String else {
            throw InstallError.notAnUpdate("bundle \(identifier ?? "missing")")
        }
        guard version == release.version,
              AppVersion.isNewer(version, than: old["CFBundleShortVersionString"] as? String ?? "0") else {
            throw InstallError.notAnUpdate("version \(version)")
        }
        try checkSignature(app, against: target)
    }

    /// Throws unless `app` has a valid signature that satisfies the designated requirement of `reference`.
    static func checkSignature(_ app: URL, against reference: URL) throws {
        var referenceCode: SecStaticCode?
        var requirement: SecRequirement?
        var candidate: SecStaticCode?
        guard SecStaticCodeCreateWithPath(reference as CFURL, [], &referenceCode) == errSecSuccess, let referenceCode,
              SecCodeCopyDesignatedRequirement(referenceCode, [], &requirement) == errSecSuccess,
              SecStaticCodeCreateWithPath(app as CFURL, [], &candidate) == errSecSuccess, let candidate
        else { throw InstallError.signatureMismatch }
        let flags = SecCSFlags(rawValue: SecCSFlags.RawValue(kSecCSCheckAllArchitectures | kSecCSStrictValidate
                                                                 | kSecCSCheckNestedCode))
        guard SecStaticCodeCheckValidity(candidate, flags, requirement) == errSecSuccess else {
            throw InstallError.signatureMismatch
        }
    }

    /// Puts `staged` where `target` is. When the folder is writable, the copy goes next to the target
    /// first so `replaceItemAt` can exchange the two on one volume and a failure leaves the old app as
    /// it was. Otherwise macOS asks for an administrator password.
    static func swap(_ staged: URL, into target: URL) async throws {
        let folder = target.deletingLastPathComponent()
        let files = FileManager.default
        if files.isWritableFile(atPath: folder.path) && files.isWritableFile(atPath: target.path) {
            let neighbour = folder.appendingPathComponent(".Shepit-update.app")
            try? files.removeItem(at: neighbour)
            defer { try? files.removeItem(at: neighbour) }
            try await run("/usr/bin/ditto", [staged.path, neighbour.path])
            _ = try files.replaceItemAt(target, withItemAt: neighbour)
        } else {
            do {
                try await run("/usr/bin/osascript", [
                    "-e", "on run argv",
                    "-e", "do shell script \"/bin/rm -rf \" & quoted form of item 1 of argv & \" && /usr/bin/ditto \" & quoted form of item 2 of argv & \" \" & quoted form of item 1 of argv with administrator privileges",
                    "-e", "end run",
                    target.path, staged.path,
                ])
            } catch InstallError.tool(_, let output) where output.contains("-128") {
                throw InstallError.cancelled
            }
        }
    }

    @discardableResult
    static func run(_ tool: String, _ arguments: [String]) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: tool)
            process.arguments = arguments
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = pipe
            process.terminationHandler = { process in
                let output = String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                if process.terminationStatus == 0 {
                    continuation.resume(returning: output)
                } else {
                    continuation.resume(throwing: InstallError.tool(URL(fileURLWithPath: tool).lastPathComponent, output))
                }
            }
            do { try process.run() } catch { continuation.resume(throwing: error) }
        }
    }
}

extension Result where Failure == Error {
    init(catching body: () async throws -> Success) async {
        do { self = .success(try await body()) } catch { self = .failure(error) }
    }
}

/// One download with progress reports, written to `destination`.
private final class Downloader: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
    private let destination: URL
    private let expectedSize: Int
    private let progress: @Sendable (Double) -> Void
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Error>?

    private init(destination: URL, expectedSize: Int, progress: @escaping @Sendable (Double) -> Void) {
        self.destination = destination
        self.expectedSize = expectedSize
        self.progress = progress
    }

    static func download(_ url: URL, to destination: URL, expectedSize: Int,
                         progress: @escaping @Sendable (Double) -> Void) async throws {
        let downloader = Downloader(destination: destination, expectedSize: expectedSize, progress: progress)
        let session = URLSession(configuration: .ephemeral, delegate: downloader, delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            downloader.lock.withLock { downloader.continuation = continuation }
            session.downloadTask(with: url).resume()
        }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
                    totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : Int64(expectedSize)
        if total > 0 { progress(min(1, Double(totalBytesWritten) / Double(total))) }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        do {
            if let http = downloadTask.response as? HTTPURLResponse, http.statusCode != 200 {
                throw ReleaseFeed.FeedError.http(http.statusCode)
            }
            try FileManager.default.moveItem(at: location, to: destination)
            finish(nil)
        } catch {
            finish(error)
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error { finish(error) }
    }

    private func finish(_ error: Error?) {
        let continuation = lock.withLock {
            defer { self.continuation = nil }
            return self.continuation
        }
        if let error { continuation?.resume(throwing: error) } else { continuation?.resume() }
    }
}
