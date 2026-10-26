import XCTest
@testable import ShepitCore

final class JSONExtractionTests: XCTestCase {
    func testPlainObject() {
        let obj = JSONExtraction.firstObject(in: #"{"text": "Hi", "language": "en", "new_terms": []}"#)
        XCTAssertEqual(obj?["text"] as? String, "Hi")
    }

    func testFencedObjectWithProse() {
        let reply = "Sure, here it is:\n```json\n{\"text\": \"Привіт\", \"language\": \"uk\", \"new_terms\": [\"Vercel\"]}\n```"
        let obj = JSONExtraction.firstObject(in: reply)
        XCTAssertEqual(obj?["text"] as? String, "Привіт")
        XCTAssertEqual(obj?["new_terms"] as? [String], ["Vercel"])
    }

    func testRawNewlineInsideString() {
        let reply = "{\"text\": \"line one\nline two\", \"language\": \"en\", \"new_terms\": []}"
        let obj = JSONExtraction.firstObject(in: reply)
        XCTAssertEqual(obj?["text"] as? String, "line one\nline two")
    }

    func testNoObject() {
        XCTAssertNil(JSONExtraction.firstObject(in: "just text"))
    }
}

final class PromptBuilderTests: XCTestCase {
    func testSystemPromptListsEveryProfile() {
        let prompt = PromptBuilder.systemPrompt()
        for p in BuiltInProfiles.all {
            XCTAssertTrue(prompt.contains("### \(p.id)"), "missing \(p.id)")
        }
        XCTAssertTrue(prompt.contains("\"new_terms\""))
    }

    func testUserMessageCarriesContextAndTranscript() {
        let ctx = TargetContext(bundleID: "com.apple.Terminal", appName: "Terminal", windowTitle: "Claude Code")
        let req = FormatRequest(transcript: "зроби рефакторинг", languageHint: "uk", mode: .prompt,
                                profile: BuiltInProfiles.claudeCodeTerminal, context: ctx, dictionary: ["Shepit"])
        let m = PromptBuilder.userMessage(req)
        XCTAssertTrue(m.contains("\"profile\":\"claude-code-terminal\""))
        XCTAssertTrue(m.contains("\"mode\":\"prompt\""))
        XCTAssertTrue(m.contains("Dictionary (preferred spellings): Shepit"))
        XCTAssertTrue(m.hasSuffix("Transcript:\nзроби рефакторинг"))
    }

    func testParseReply() {
        let parsed = PromptBuilder.parseReply(#"{"text":"Hello","language":"en","new_terms":["X"]}"#)
        XCTAssertEqual(parsed?.text, "Hello")
        XCTAssertEqual(parsed?.language, "en")
        XCTAssertEqual(parsed?.newTerms, ["X"])
    }
}
