import XCTest
@testable import ShepitCore

private final class RunLog: @unchecked Sendable {
    private let lock = NSLock()
    private var runs: [(String, [String])] = []
    var all: [(String, [String])] { lock.withLock { runs } }
    func add(_ tool: String, _ args: [String]) { lock.withLock { runs.append((tool, args)) } }
}

private struct RunnerFailure: Error {}

final class AccessibilityResetTests: XCTestCase {
    func testArgumentsAreFixedForAValidBundleID() {
        XCTAssertEqual(AccessibilityReset.tool, "/usr/bin/tccutil")
        XCTAssertEqual(AccessibilityReset.arguments(bundleID: "com.themarchenko.shepit"),
                       ["reset", "Accessibility", "com.themarchenko.shepit"])
    }

    func testArgumentsRefuseMissingOrEmptyBundleID() {
        // Without an id tccutil would reset the permission of every app.
        XCTAssertNil(AccessibilityReset.arguments(bundleID: nil))
        XCTAssertNil(AccessibilityReset.arguments(bundleID: ""))
        XCTAssertNil(AccessibilityReset.arguments(bundleID: "   "))
    }

    func testArgumentsRefuseOptionLikeOrMalformedIDs() {
        for bad in ["-h", "All", "com.x y", "com.x;ls -la /", "com..x", "com.x\n", ".com.x"] {
            XCTAssertNil(AccessibilityReset.arguments(bundleID: bad), bad)
        }
    }

    func testPerformRunsTccutilOnce() async {
        let log = RunLog()
        let ok = await AccessibilityReset.perform(bundleID: "com.themarchenko.shepit") { tool, args in
            log.add(tool, args)
            return ""
        }
        XCTAssertTrue(ok)
        XCTAssertEqual(log.all.count, 1)
        XCTAssertEqual(log.all.first?.0, "/usr/bin/tccutil")
        XCTAssertEqual(log.all.first?.1, ["reset", "Accessibility", "com.themarchenko.shepit"])
    }

    func testPerformNeverRunsForInvalidID() async {
        let log = RunLog()
        for bad in [nil, "", "-h", "All"] as [String?] {
            let ok = await AccessibilityReset.perform(bundleID: bad) { tool, args in
                log.add(tool, args)
                return ""
            }
            XCTAssertFalse(ok)
        }
        XCTAssertEqual(log.all.count, 0)
    }

    func testPerformReportsRunnerFailure() async {
        let ok = await AccessibilityReset.perform(bundleID: "com.themarchenko.shepit") { _, _ in throw RunnerFailure() }
        XCTAssertFalse(ok)
    }
}
