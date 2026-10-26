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

final class PassthroughFormatterTests: XCTestCase {
    func testCleanCollapsesWhitespaceAndCapitalizes() {
        XCTAssertEqual(PassthroughFormatter.clean("  добре,   зараз\nподивлюсь "), "Добре, зараз подивлюсь")
        XCTAssertEqual(PassthroughFormatter.clean(""), "")
    }
}

final class ProfileResolverTests: XCTestCase {
    func testTerminalWithClaudeTitle() {
        let ctx = TargetContext(bundleID: "com.apple.Terminal", windowTitle: "✳ Claude Code — shepit")
        XCTAssertEqual(ProfileResolver.resolve(ctx).id, "claude-code-terminal")
    }

    func testTerminalWithoutClaude() {
        let ctx = TargetContext(bundleID: "com.googlecode.iterm2", windowTitle: "zsh")
        XCTAssertEqual(ProfileResolver.resolve(ctx).id, "terminal")
    }

    func testAIApps() {
        XCTAssertEqual(ProfileResolver.resolve(TargetContext(bundleID: "com.anthropic.claudefordesktop")).id, "ai-chat")
        XCTAssertEqual(ProfileResolver.resolve(TargetContext(bundleID: "com.google.Chrome", url: "https://claude.ai/new")).id, "ai-chat")
        XCTAssertEqual(ProfileResolver.resolve(TargetContext(bundleID: "com.google.Chrome", url: "https://www.chatgpt.com/")).id, "ai-chat")
    }

    func testMessengerMailNotesGeneric() {
        XCTAssertEqual(ProfileResolver.resolve(TargetContext(bundleID: "ru.keepcoder.Telegram")).id, "messenger")
        XCTAssertEqual(ProfileResolver.resolve(TargetContext(bundleID: "com.apple.mail")).id, "mail")
        XCTAssertEqual(ProfileResolver.resolve(TargetContext(bundleID: "com.apple.Notes")).id, "notes")
        XCTAssertEqual(ProfileResolver.resolve(TargetContext(bundleID: "com.unknown.app")).id, "generic")
        XCTAssertEqual(ProfileResolver.resolve(TargetContext()).id, "generic")
    }
}

final class ModeResolverTests: XCTestCase {
    func testPromptModeOnlyForLongAITargets() {
        let ai = BuiltInProfiles.aiChat
        XCTAssertEqual(ModeResolver.resolve(requested: .clean, profile: ai, wordCount: 39, promptModeMinWords: 40), .clean)
        XCTAssertEqual(ModeResolver.resolve(requested: .clean, profile: ai, wordCount: 40, promptModeMinWords: 40), .prompt)
        XCTAssertEqual(ModeResolver.resolve(requested: .clean, profile: BuiltInProfiles.messenger, wordCount: 200, promptModeMinWords: 40), .clean)
        XCTAssertEqual(ModeResolver.resolve(requested: .raw, profile: ai, wordCount: 200, promptModeMinWords: 40), .raw)
        XCTAssertEqual(ModeResolver.resolve(requested: .prompt, profile: BuiltInProfiles.messenger, wordCount: 3, promptModeMinWords: 40), .prompt)
    }

    func testWordCount() {
        XCTAssertEqual(ModeResolver.wordCount("one two\nthree   four"), 4)
    }
}
