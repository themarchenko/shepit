import XCTest
@testable import ShepitCore

final class UpdateTests: XCTestCase {
    func testParsesLatestRelease() throws {
        let json = """
        {"tag_name": "v0.3.0", "html_url": "https://github.com/themarchenko/shepit/releases/tag/v0.3.0",
         "draft": false, "prerelease": false,
         "assets": [
           {"name": "notes.txt", "browser_download_url": "https://example.com/notes.txt", "size": 10},
           {"name": "Shepit.dmg", "size": 29012156,
            "browser_download_url": "https://github.com/themarchenko/shepit/releases/download/v0.3.0/Shepit.dmg"}
         ]}
        """
        let release = try ReleaseFeed.parse(Data(json.utf8))
        XCTAssertEqual(release.version, "0.3.0")
        XCTAssertEqual(release.diskImageURL.lastPathComponent, "Shepit.dmg")
        XCTAssertEqual(release.diskImageSize, 29012156)
        XCTAssertEqual(release.pageURL.absoluteString, "https://github.com/themarchenko/shepit/releases/tag/v0.3.0")
    }

    func testReleaseWithoutDiskImageIsRejected() {
        let json = """
        {"tag_name": "v0.2.0", "html_url": "https://github.com/themarchenko/shepit/releases/tag/v0.2.0",
         "assets": [{"name": "Shepit-0.2.0.zip", "browser_download_url": "https://example.com/a.zip", "size": 1}]}
        """
        XCTAssertThrowsError(try ReleaseFeed.parse(Data(json.utf8)))
    }
}

/// Installs real disk images over a scratch copy of the app. Needs prepared files, so it only runs when
/// SHEPIT_UPDATE_E2E names a folder with `Shepit.app` (the copy to replace) and disk images `good.dmg`,
/// `adhoc.dmg` and `same-version.dmg`. See docs/DECISIONS.md, 2026-10-26.
final class UpdateInstallerEndToEndTests: XCTestCase {
    private var folder: URL!

    override func setUpWithError() throws {
        guard let path = ProcessInfo.processInfo.environment["SHEPIT_UPDATE_E2E"] else {
            throw XCTSkip("set SHEPIT_UPDATE_E2E to run")
        }
        folder = URL(fileURLWithPath: path)
    }

    private func release(_ image: String, version: String) -> AppRelease {
        AppRelease(version: version, pageURL: URL(string: "https://example.com")!,
                   diskImageURL: folder.appendingPathComponent(image), diskImageSize: 0)
    }

    private func installedVersion() -> String? {
        NSDictionary(contentsOf: folder.appendingPathComponent("Shepit.app/Contents/Info.plist"))?["CFBundleShortVersionString"] as? String
    }

    func testRejectsAnotherSigner() async throws {
        let before = installedVersion()
        do {
            _ = try await UpdateInstaller.install(release("adhoc.dmg", version: "9.0.0"),
                                                  over: folder.appendingPathComponent("Shepit.app")) { _ in }
            XCTFail("an ad hoc build was installed")
        } catch UpdateInstaller.InstallError.signatureMismatch {}
        XCTAssertEqual(installedVersion(), before)
    }

    func testRejectsSameVersion() async throws {
        do {
            _ = try await UpdateInstaller.install(release("same-version.dmg", version: installedVersion() ?? ""),
                                                  over: folder.appendingPathComponent("Shepit.app")) { _ in }
            XCTFail("the same version was installed")
        } catch UpdateInstaller.InstallError.notAnUpdate {}
    }

    func testZInstallsSignedRelease() async throws {
        let phases = PhaseLog()
        let app = try await UpdateInstaller.install(release("good.dmg", version: "9.0.0"),
                                                    over: folder.appendingPathComponent("Shepit.app")) { phases.add($0) }
        XCTAssertEqual(installedVersion(), "9.0.0")
        XCTAssertNil(try? FileManager.default.extendedAttributeNames(app).first { $0 == "com.apple.quarantine" })
        XCTAssertEqual(phases.last, .installing)
    }
}

/// The real thing: reads the latest release from GitHub and installs it over a copy of an older
/// release-signed app. SHEPIT_UPDATE_LIVE names the folder with that `Shepit.app`.
final class UpdateFromGitHubTests: XCTestCase {
    func testInstallsLatestRelease() async throws {
        guard let path = ProcessInfo.processInfo.environment["SHEPIT_UPDATE_LIVE"] else {
            throw XCTSkip("set SHEPIT_UPDATE_LIVE to run")
        }
        let app = URL(fileURLWithPath: path).appendingPathComponent("Shepit.app")
        let release = try await ReleaseFeed.latest()
        let phases = PhaseLog()
        _ = try await UpdateInstaller.install(release, over: app) { phases.add($0) }
        let installed = NSDictionary(contentsOf: app.appendingPathComponent("Contents/Info.plist"))
        XCTAssertEqual(installed?["CFBundleShortVersionString"] as? String, release.version)
        XCTAssertEqual(phases.last, .installing)
    }
}

private final class PhaseLog: @unchecked Sendable {
    private let lock = NSLock()
    private var phases: [UpdateInstaller.Phase] = []
    func add(_ phase: UpdateInstaller.Phase) { lock.withLock { phases.append(phase) } }
    var last: UpdateInstaller.Phase? { lock.withLock { phases.last } }
}

private extension FileManager {
    func extendedAttributeNames(_ url: URL) throws -> [String] {
        let size = listxattr(url.path, nil, 0, 0)
        guard size > 0 else { return [] }
        var buffer = [CChar](repeating: 0, count: size)
        listxattr(url.path, &buffer, size, 0)
        return buffer.split(separator: 0).map { String(decoding: $0.map { UInt8(bitPattern: $0) }, as: UTF8.self) }
    }
}
