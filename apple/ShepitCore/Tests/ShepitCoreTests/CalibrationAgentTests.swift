import XCTest
@testable import ShepitCore

final class CalibrationAgentTests: XCTestCase {
    func testWorkDirectoryFreshAndNotWorkerDirs() {
        let a = CalibrationAgents.freshWorkDirectory()
        let b = CalibrationAgents.freshWorkDirectory()
        defer { try? FileManager.default.removeItem(at: a); try? FileManager.default.removeItem(at: b) }
        XCTAssertNotEqual(a, b)
        let temp = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().path
        XCTAssertTrue(a.resolvingSymlinksInPath().path.hasPrefix(temp), a.path)
        XCTAssertTrue(b.resolvingSymlinksInPath().path.hasPrefix(temp), b.path)
        for dir in [a, b] {
            XCTAssertFalse(dir.path.contains("claude-work"))
            XCTAssertFalse(dir.path.contains("codex-work"))
        }
    }

    func testFactoryNilWhenOffOrMissingExecutable() {
        XCTAssertNil(CalibrationAgents.make(provider: .off, claudeExecutable: "/bin/echo", claudeModel: "haiku",
                                            codexExecutable: "/bin/echo"))
        XCTAssertNil(CalibrationAgents.make(provider: .claudeCode, claudeExecutable: nil, claudeModel: "haiku",
                                            codexExecutable: "/bin/echo"))
        XCTAssertNil(CalibrationAgents.make(provider: .codex, claudeExecutable: "/bin/echo", claudeModel: "haiku",
                                            codexExecutable: nil))
        XCTAssertNotNil(CalibrationAgents.make(provider: .claudeCode, claudeExecutable: "/bin/echo", claudeModel: "haiku",
                                               codexExecutable: nil))
        XCTAssertNotNil(CalibrationAgents.make(provider: .codex, claudeExecutable: nil, claudeModel: "haiku",
                                               codexExecutable: "/bin/echo"))
    }
}
