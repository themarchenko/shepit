import AppKit
import XCTest
@testable import ShepitCore

final class DictationOutcomeTests: XCTestCase {
    func testUneditedPasteHasNoFinalText() {
        let f = OutcomeFields(formattedText: "a", sentText: "a", outcome: .inserted(.paste))
        XCTAssertNil(f.finalText)
        XCTAssertEqual(f.insertMethod, "paste")
        XCTAssertEqual(f.status, "inserted")
    }

    func testEditedPasteKeepsFinalText() {
        let f = OutcomeFields(formattedText: "a", sentText: "b", outcome: .inserted(.paste))
        XCTAssertEqual(f.finalText, "b")
    }

    func testWhitespaceEditCountsAsEdit() {
        let f = OutcomeFields(formattedText: "a", sentText: "a ", outcome: .inserted(.paste))
        XCTAssertEqual(f.finalText, "a ")
    }

    func testClipboardOnlyStatus() {
        let f = OutcomeFields(formattedText: "a", sentText: "a", outcome: .inserted(.clipboardOnly))
        XCTAssertEqual(f.insertMethod, "clipboardOnly")
        XCTAssertEqual(f.status, "clipboard")
    }

    func testDiscardedHasNoFinalTextEvenWhenEdited() {
        let f = OutcomeFields(formattedText: "a", sentText: "b", outcome: .discarded)
        XCTAssertNil(f.finalText)
        XCTAssertEqual(f.insertMethod, "none")
        XCTAssertEqual(f.status, "discarded")
    }

    func testFailedKeepsEditedText() {
        let f = OutcomeFields(formattedText: "a", sentText: "b", outcome: .failed)
        XCTAssertEqual(f.finalText, "b")
        XCTAssertEqual(f.insertMethod, "none")
        XCTAssertEqual(f.status, "failed")
    }

    func testReviewPolicyIsSettingOrShift() {
        XCTAssertFalse(ReviewPolicy.shouldReview(setting: false, shiftHeld: false))
        XCTAssertTrue(ReviewPolicy.shouldReview(setting: true, shiftHeld: false))
        XCTAssertTrue(ReviewPolicy.shouldReview(setting: false, shiftHeld: true))
        XCTAssertTrue(ReviewPolicy.shouldReview(setting: true, shiftHeld: true))
    }

    func testDisplayTextPrefersFinalText() {
        var r = DictationRecord(bundleID: nil, appName: nil, windowTitle: nil, profile: "p", mode: "m", engine: "e",
                                rawText: "raw", formattedText: "formatted", language: nil, audioSeconds: 1,
                                sttMs: 1, llmMs: 0, formatterSource: "none", insertMethod: "paste", status: "inserted")
        XCTAssertEqual(r.displayText, "formatted")
        r.finalText = "edited"
        XCTAssertEqual(r.displayText, "edited")
    }
}

@MainActor
final class DictationReviewTests: XCTestCase {
    private var pasteboard: NSPasteboard!
    private var storeURL: URL!
    private var history: HistoryStore!
    private var session: DictationSession!
    private var pastes = 0
    private var boardAtPaste: String?
    private var onPaste: (() -> Void)?

    override func setUp() async throws {
        // A named board, so the tests never touch the user's clipboard.
        pasteboard = NSPasteboard(name: NSPasteboard.Name("com.themarchenko.shepit.tests.\(UUID().uuidString)"))
        pasteboard.clearContents()
        storeURL = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-test-\(UUID().uuidString).sqlite")
        history = try HistoryStore(url: storeURL)
        pastes = 0
        boardAtPaste = nil
        onPaste = nil

        let inserter = PasteInserter(pasteboard: pasteboard)
        inserter.isTrusted = { true }
        inserter.restoreDelay = .milliseconds(10)
        inserter.sendPaste = { [unowned self] in
            pastes += 1
            boardAtPaste = pasteboard.string(forType: .string)
            onPaste?()
        }
        let settings = AppSettings(defaults: UserDefaults(suiteName: UUID().uuidString)!)
        session = DictationSession(recorder: AudioRecorder(), engines: EngineManager(),
                                   formatting: FormattingService(claude: nil), inserter: inserter,
                                   history: history, settings: settings)
        inserterRef = inserter
    }

    private var inserterRef: PasteInserter!

    override func tearDown() async throws {
        pasteboard.releaseGlobally()
        for suffix in ["", "-wal", "-shm", "-journal"] {
            try? FileManager.default.removeItem(atPath: storeURL.path + suffix)
        }
    }

    private func pending(_ text: String = "formatted", secure: Bool = false) -> PendingReview {
        let context = TargetContext(bundleID: "com.example.app", appName: "Example", secureInput: secure)
        let record = DictationRecord(bundleID: context.bundleID, appName: context.appName, windowTitle: nil,
                                     profile: "p", mode: "m", engine: "e", rawText: "raw", formattedText: text,
                                     language: "en", audioSeconds: 1, sttMs: 1, llmMs: 0, formatterSource: "none",
                                     insertMethod: "", status: "")
        return PendingReview(context: context, record: record)
    }

    private func rows() throws -> [DictationRecord] { try history.recent(limit: 10) }

    func testEnterPastesEditedTextAndRecordsIt() async throws {
        session.beginReview(pending("formatted"))
        XCTAssertEqual(session.state, .reviewing(text: "formatted"))
        XCTAssertTrue(session.state.isReviewing)
        session.confirmReview("edited")
        await waitUntil { if case .done = self.session.state { return true } else { return false } }
        XCTAssertEqual(pastes, 1)
        XCTAssertEqual(boardAtPaste, "edited")
        let all = try rows()
        XCTAssertEqual(all.count, 1)
        XCTAssertEqual(all[0].finalText, "edited")
        XCTAssertEqual(all[0].status, "inserted")
        XCTAssertEqual(session.lastRecord?.displayText, "edited")
        guard case .done = session.state else { return XCTFail("state \(session.state)") }
    }

    func testEnterWithUneditedTextLeavesFinalTextNil() async throws {
        session.beginReview(pending("formatted"))
        session.confirmReview("formatted")
        await waitUntil { if case .done = self.session.state { return true } else { return false } }
        let all = try rows()
        XCTAssertEqual(all.count, 1)
        XCTAssertNil(all[0].finalText)
        XCTAssertEqual(all[0].status, "inserted")
    }

    func testEscapeTwiceWritesOneDiscardedRowAndNoPaste() async throws {
        session.beginReview(pending("formatted"))
        session.discardReview()
        session.discardReview()
        session.cancel()
        session.confirmReview("x")
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(pastes, 0)
        let all = try rows()
        XCTAssertEqual(all.count, 1)
        XCTAssertEqual(all[0].status, "discarded")
        XCTAssertNil(all[0].finalText)
        XCTAssertEqual(session.state, .idle)
    }

    func testCancelWhileReviewingDiscards() throws {
        session.beginReview(pending("formatted"))
        session.cancel()
        let all = try rows()
        XCTAssertEqual(all.count, 1)
        XCTAssertEqual(all[0].status, "discarded")
        XCTAssertEqual(session.state, .idle)
        XCTAssertEqual(pastes, 0)
    }

    func testSecureInputAfterEnterWritesNoRow() async throws {
        session.beginReview(pending("formatted", secure: true))
        session.confirmReview("edited")
        await waitUntil { if case .failed = self.session.state { return true } else { return false } }
        guard case .failed = session.state else { return XCTFail("state \(session.state)") }
        XCTAssertEqual(pastes, 0)
        let all = try rows()
        XCTAssertEqual(all.count, 0)
        XCTAssertNotEqual(session.lastRecord?.displayText, "edited")
    }

    func testEmptyEditDiscards() async throws {
        session.beginReview(pending("formatted"))
        session.confirmReview("  \n")
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(pastes, 0)
        let all = try rows()
        XCTAssertEqual(all.count, 1)
        XCTAssertEqual(all[0].status, "discarded")
        XCTAssertEqual(session.state, .idle)
    }

    func testStartAndToggleAreIgnoredWhileReviewing() {
        session.beginReview(pending("formatted"))
        session.startRecording(handsFree: false)
        XCTAssertEqual(session.state, .reviewing(text: "formatted"))
        session.toggleHandsFree()
        XCTAssertEqual(session.state, .reviewing(text: "formatted"))
    }

    func testEscDuringPasteWaitStillRecordsThePaste() async throws {
        inserterRef.restoreDelay = .milliseconds(300)
        onPaste = { [unowned self] in session.cancel() }
        session.beginReview(pending("formatted"))
        session.confirmReview("edited")
        await waitUntil { (try? self.rows().count) == 1 }
        XCTAssertEqual(pastes, 1)
        let all = try rows()
        XCTAssertEqual(all.count, 1)
        XCTAssertEqual(all[0].status, "inserted")
        XCTAssertEqual(session.state, .idle)
    }

    func testConfirmedReviewRecordsReviewedInsert() async throws {
        session.beginReview(pending("formatted"))
        session.confirmReview("edited")
        await waitUntil { if case .done = self.session.state { return true } else { return false } }
        XCTAssertEqual(session.lastInsert?.reviewed, true)
        XCTAssertEqual(session.lastInsert?.method.rawValue, try rows()[0].insertMethod)
    }

    func testDiscardedReviewRecordsNoInsert() {
        session.beginReview(pending("formatted"))
        session.discardReview()
        XCTAssertNil(session.lastInsert)
    }
}

@MainActor
final class ReviewSettingTests: XCTestCase {
    func testReviewBeforePasteDefaultsOffAndPersists() {
        let name = UUID().uuidString
        let defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        let first = AppSettings(defaults: defaults)
        XCTAssertFalse(first.reviewBeforePaste)
        first.reviewBeforePaste = true
        XCTAssertTrue(AppSettings(defaults: defaults).reviewBeforePaste)
    }
}
