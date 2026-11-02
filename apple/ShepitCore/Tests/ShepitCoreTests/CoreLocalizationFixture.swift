import XCTest
@testable import ShepitCore

/// Compiles the core String Catalog the way Xcode does, so tests can read Ukrainian. A plain `swift test`
/// copies the catalog uncompiled, which leaves only English to look up.
enum CoreLocalizationFixture {
    private static let catalog = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent()
        .appendingPathComponent("Sources/ShepitCore/Resources/Localizable.xcstrings")

    /// Compiled once per test run.
    private static let compiled: Result<Bundle, Error> = Result {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("shepit-loc-\(UUID().uuidString)", isDirectory: true)
        let bundleURL = dir.appendingPathComponent("Loc.bundle", isDirectory: true)
        try FileManager.default.createDirectory(at: bundleURL, withIntermediateDirectories: true)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
        process.arguments = ["xcstringstool", "compile", catalog.path, "--output-directory", bundleURL.path]
        let errors = Pipe()
        process.standardError = errors
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            let text = String(data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            throw NSError(domain: "CoreLocalizationFixture", code: Int(process.terminationStatus),
                          userInfo: [NSLocalizedDescriptionKey: "xcstringstool compile failed: \(text)"])
        }
        guard let bundle = Bundle(path: bundleURL.path) else {
            throw NSError(domain: "CoreLocalizationFixture", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "compiled catalog is not a bundle"])
        }
        return bundle
    }

    /// Call from `setUp() throws`. A missing `xcrun` or a failed compile fails the test, it never skips.
    static func install() throws {
        CoreLocalization.overrideBundle = try compiled.get()
    }

    static func reset() {
        CoreLocalization.override = nil
        CoreLocalization.overrideBundle = nil
    }
}
