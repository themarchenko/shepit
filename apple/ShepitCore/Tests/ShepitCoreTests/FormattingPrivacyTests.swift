import XCTest
@testable import ShepitCore

private final class CallCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var n = 0
    var count: Int { lock.lock(); defer { lock.unlock() }; return n }
    func hit() { lock.lock(); n += 1; lock.unlock() }
}

private struct StubFormatter: ShepitCore.Formatter {
    let calls: CallCounter
    func format(_ request: FormatRequest) async throws -> FormatResult {
        calls.hit()
        return FormatResult(text: "from agent", source: .claude)
    }
    func warmUp() async {}
}

final class SecureFieldFormattingTests: XCTestCase {
    private let twentyWords = (1...20).map { _ in "word" }.joined(separator: "   ") + " Shepit"

    private func request(secure: Bool, mode: Mode = .clean, transcript: String? = nil) -> FormatRequest {
        let ctx = TargetContext(bundleID: "com.apple.Notes", appName: "Notes", windowTitle: "", secureInput: secure)
        return FormatRequest(transcript: transcript ?? twentyWords, languageHint: "en", mode: mode,
                             profile: BuiltInProfiles.notes, context: ctx, dictionary: ["Shepit"])
    }

    func testSecureFieldSkipsTheAgent() async {
        let calls = CallCounter()
        let service = FormattingService(claude: StubFormatter(calls: calls))
        let result = await service.format(request(secure: true))
        XCTAssertEqual(result.source, .passthrough)
        XCTAssertEqual(result.text, "Word word word word word word word word word word word word word word word word word word word word Shepit")
        XCTAssertEqual(calls.count, 0)
    }

    func testNonSecureFieldCallsTheAgentOnce() async {
        let calls = CallCounter()
        let service = FormattingService(claude: StubFormatter(calls: calls))
        let result = await service.format(request(secure: false))
        XCTAssertEqual(result.source, .claude)
        XCTAssertEqual(calls.count, 1)
    }

    func testRawAndFastPathKeepTheirSourceWhenSecure() async {
        let calls = CallCounter()
        let service = FormattingService(claude: StubFormatter(calls: calls))
        let raw = await service.format(request(secure: true, mode: .raw))
        XCTAssertEqual(raw.source, .passthrough)
        let fast = await service.format(request(secure: true, transcript: "short text"))
        XCTAssertEqual(fast.source, .fastPath)
        XCTAssertEqual(calls.count, 0)
    }
}

final class FormatterErrorLogTests: XCTestCase {
    private let reply = "SECRETREPLY " + String(repeating: "x", count: 400)

    func testBadReplyDescriptionsCarryOnlyTheLength() {
        XCTAssertEqual(reply.utf8.count, 412)
        let claude = WorkerError.badReply(bytes: reply.utf8.count).localizedDescription
        let codex = CodexError.badReply(bytes: reply.utf8.count).localizedDescription
        XCTAssertEqual(claude, "Claude reply was not JSON (412 bytes)")
        XCTAssertEqual(codex, "Codex reply was not JSON (412 bytes)")
        XCTAssertFalse(claude.contains("SECRETREPLY"))
        XCTAssertFalse(codex.contains("SECRETREPLY"))
    }

    func testLogKindKeepsTheLengthAndDropsText() {
        let kind = FormattingService.logKind(WorkerError.badReply(bytes: 412))
        XCTAssertTrue(kind.contains("412"))
        XCTAssertFalse(kind.contains("SECRETREPLY"))
        XCTAssertEqual(FormattingService.logKind(CodexError.badReply(bytes: 412)), "badReply(412 bytes)")
    }

    func testLogKindDropsServerText() {
        XCTAssertFalse(FormattingService.logKind(WorkerError.apiError("secret words")).contains("secret"))
        XCTAssertFalse(FormattingService.logKind(CodexError.rpc("secret words")).contains("secret"))
        XCTAssertFalse(FormattingService.logKind(CodexError.turnFailed("secret words")).contains("secret"))
        XCTAssertEqual(FormattingService.logKind(WorkerError.exited(1)), "exited(1)")
        XCTAssertEqual(FormattingService.logKind(WorkerError.timeout), "timeout")
        XCTAssertEqual(FormattingService.logKind(CodexError.paused(until: Date())), "paused")
    }

    func testLogKindForUnknownErrorIsTheTypeName() {
        struct Odd: Error { let detail = "secret words" }
        let kind = FormattingService.logKind(Odd())
        XCTAssertTrue(kind.contains("Odd"))
        XCTAssertFalse(kind.contains("secret"))
    }
}
