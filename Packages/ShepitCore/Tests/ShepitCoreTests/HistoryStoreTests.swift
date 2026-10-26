import XCTest
@testable import ShepitCore

final class HistoryStoreTests: XCTestCase {
    func testSaveAndFetch() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-test-\(UUID().uuidString).sqlite")
        defer { try? FileManager.default.removeItem(at: url) }
        let store = try HistoryStore(url: url)
        let record = DictationRecord(bundleID: "com.apple.Terminal", appName: "Terminal", windowTitle: "Claude Code",
                                     profile: "claude-code-terminal", mode: "prompt", engine: "parakeet-v3",
                                     rawText: "raw", formattedText: "formatted", language: "uk", audioSeconds: 3.2,
                                     sttMs: 400, llmMs: 1500, formatterSource: "claude", insertMethod: "paste", status: "inserted")
        let saved = try store.save(record)
        XCTAssertNotNil(saved.id)
        try store.updateFinalText(id: saved.id!, text: "final")
        let recent = try store.recent(limit: 10)
        XCTAssertEqual(recent.count, 1)
        XCTAssertEqual(recent[0].finalText, "final")
        XCTAssertEqual(try store.count(), 1)
    }
}
