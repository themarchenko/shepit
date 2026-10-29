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

    private func makeRecord(_ text: String) -> DictationRecord {
        DictationRecord(bundleID: nil, appName: nil, windowTitle: nil, profile: "p", mode: "m", engine: "e",
                        rawText: text, formattedText: text, finalText: text, language: "en", audioSeconds: 1,
                        sttMs: 1, llmMs: 0, formatterSource: "none", insertMethod: "paste", status: "inserted")
    }

    private func tempStoreURL() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("shepit-test-\(UUID().uuidString).sqlite")
    }

    private func removeStoreFiles(_ url: URL) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            try? FileManager.default.removeItem(atPath: url.path + suffix)
        }
    }

    func testClearDeletesEveryRow() throws {
        let url = tempStoreURL()
        defer { removeStoreFiles(url) }
        let store = try HistoryStore(url: url)
        for i in 0..<3 { try store.save(makeRecord("row \(i)")) }
        XCTAssertEqual(try store.count(), 3)
        try store.clear()
        XCTAssertEqual(try store.count(), 0)
        XCTAssertTrue(try store.recent(limit: 10).isEmpty)
    }

    func testSecureDeleteIsOn() throws {
        let url = tempStoreURL()
        defer { removeStoreFiles(url) }
        let store = try HistoryStore(url: url)
        XCTAssertTrue(try store.secureDeleteEnabled())
    }

    func testClearedTextIsNotInTheFile() throws {
        let url = tempStoreURL()
        defer { removeStoreFiles(url) }
        let marker = "zq-marker-\(UUID().uuidString)"
        let store = try HistoryStore(url: url)
        try store.save(makeRecord(marker))
        try store.save(makeRecord("other row"))
        try store.clear()
        // The store stays open: a WAL file, if any, is still in play. Read every file SQLite may use.
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let path = url.path + suffix
            guard let data = FileManager.default.contents(atPath: path) else { continue }
            XCTAssertNil(data.range(of: Data(marker.utf8)), "marker found in \(path)")
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
    }
}
