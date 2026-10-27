import AppKit
import XCTest
@testable import ShepitCore

@MainActor
final class PasteInserterTests: XCTestCase {
    private var pasteboard: NSPasteboard!

    override func setUp() {
        // A named board, so the tests never touch the user's clipboard.
        pasteboard = NSPasteboard(name: NSPasteboard.Name("com.themarchenko.shepit.tests.\(UUID().uuidString)"))
        pasteboard.clearContents()
    }

    override func tearDown() {
        pasteboard.releaseGlobally()
    }

    func testDictationIsMarkedTransientAndConcealed() {
        PasteInserter.writeDictation("secret words", to: pasteboard)
        let types = pasteboard.types ?? []
        XCTAssertTrue(types.contains(PasteInserter.transientType))
        XCTAssertTrue(types.contains(PasteInserter.concealedType))
        XCTAssertEqual(pasteboard.string(forType: .string), "secret words")
    }

    func testRestoredContentsAreNotMarked() async throws {
        pasteboard.setString("mine", forType: .string)
        let inserter = PasteInserter(pasteboard: pasteboard)
        inserter.isTrusted = { true }
        inserter.sendPaste = {}
        inserter.restoreDelay = .milliseconds(10)
        _ = try await inserter.insert("dictated", context: TargetContext())
        XCTAssertEqual(pasteboard.string(forType: .string), "mine")
        XCTAssertFalse((pasteboard.types ?? []).contains(PasteInserter.transientType))
        XCTAssertFalse((pasteboard.types ?? []).contains(PasteInserter.concealedType))
    }

    func testClipboardIsRestoredAfterTheDelayWhenCancelledDuringThePasteWait() async throws {
        pasteboard.setString("mine", forType: .string)
        let inserter = PasteInserter(pasteboard: pasteboard)
        inserter.isTrusted = { true }
        inserter.restoreDelay = .milliseconds(300)
        let pasted = expectation(description: "paste sent")
        inserter.sendPaste = { pasted.fulfill() }
        let task = Task { @MainActor in
            try await inserter.insert("dictated", context: TargetContext())
        }
        await fulfillment(of: [pasted], timeout: 5)
        task.cancel()
        _ = await task.result
        // The paste keys are already sent, so the target must still find the dictated text.
        XCTAssertEqual(pasteboard.string(forType: .string), "dictated")
        try await Task.sleep(for: .milliseconds(900))
        XCTAssertEqual(pasteboard.string(forType: .string), "mine")
    }

    func testAnInsertRightAfterACancelStillRestoresTheUsersClipboard() async throws {
        pasteboard.setString("mine", forType: .string)
        let inserter = PasteInserter(pasteboard: pasteboard)
        inserter.isTrusted = { true }
        inserter.restoreDelay = .milliseconds(300)
        let pasted = expectation(description: "paste sent")
        inserter.sendPaste = { pasted.fulfill() }
        let first = Task { @MainActor in
            try await inserter.insert("first", context: TargetContext())
        }
        await fulfillment(of: [pasted], timeout: 5)
        first.cancel()
        _ = await first.result
        inserter.sendPaste = {}
        _ = try await inserter.insert("second", context: TargetContext())
        XCTAssertEqual(pasteboard.string(forType: .string), "mine")
    }
}
