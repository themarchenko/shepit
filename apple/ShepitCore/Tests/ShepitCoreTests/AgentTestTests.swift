import XCTest
@testable import ShepitCore

private final class Captured: @unchecked Sendable {
    private let lock = NSLock()
    private var requests: [FormatRequest] = []
    var all: [FormatRequest] { lock.withLock { requests } }
    func add(_ r: FormatRequest) { lock.withLock { requests.append(r) } }
}

private struct CapturingFormatter: ShepitCore.Formatter {
    let captured: Captured
    var error: Error?
    func format(_ request: FormatRequest) async throws -> FormatResult {
        captured.add(request)
        if let error { throw error }
        return FormatResult(text: "ok", source: .claude)
    }
    func warmUp() async {}
}

final class AgentTestTests: XCTestCase {
    func testAgentTestSendsOnlyTheFixedSample() async throws {
        let captured = Captured()
        let service = FormattingService(claude: CapturingFormatter(captured: captured))
        let result = await service.testAgent()
        XCTAssertEqual(result, .passed)
        // The sample is far below fastPathMaxWords; the test must still reach the agent.
        XCTAssertEqual(captured.all.count, 1)
        let r = try XCTUnwrap(captured.all.first)
        XCTAssertEqual(r.transcript, FormattingService.doctorSample)
        XCTAssertNil(r.context.appName)
        XCTAssertNil(r.context.windowTitle)
        XCTAssertNil(r.context.textBeforeCaret)
        XCTAssertNil(r.context.selectedText)
        XCTAssertTrue(r.dictionary.isEmpty)
        XCTAssertTrue(r.examples.isEmpty)
    }

    func testAgentTestReportsTheErrorKind() async {
        let captured = Captured()
        let service = FormattingService(claude: CapturingFormatter(captured: captured, error: CodexError.paused(until: Date())))
        let result = await service.testAgent()
        XCTAssertEqual(result, .failed(kind: "paused"))
    }

    func testAgentTestWithoutAgent() async {
        let service = FormattingService(claude: nil)
        let result = await service.testAgent()
        XCTAssertEqual(result, .noAgent)
    }
}
