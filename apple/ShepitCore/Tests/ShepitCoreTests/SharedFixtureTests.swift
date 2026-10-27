import XCTest
@testable import ShepitCore

/// Runs every case of the shared fixtures against the real function. The minimum counts are the number of
/// assertions migrated from the older per-platform tests, so an emptied fixture cannot pass.
final class SharedFixtureTests: XCTestCase {
    // DictionaryCorrector.correct(_ text: String, dictionary: [String]) -> String
    func testDictionaryCorrection() throws {
        let f: FixtureFile<DictionaryCorrectionInput, String> = try SharedFixtures.load("dictionary-correction.json", minimumCases: 23)
        for c in f.cases {
            XCTAssertEqual(DictionaryCorrector.correct(c.input.text, dictionary: c.input.dictionary), c.expected, c.name)
        }
    }

    // DictionaryLearning.newEntries(from reported: [String], existing: [String]) -> [String]
    func testDictionaryLearning() throws {
        let f: FixtureFile<DictionaryLearningInput, [String]> = try SharedFixtures.load("dictionary-learning.json", minimumCases: 1)
        for c in f.cases {
            XCTAssertEqual(DictionaryLearning.newEntries(from: c.input.reported, existing: c.input.existing), c.expected, c.name)
        }
    }

    // HotkeyGesture.keyDown(at:) / keyUp(at:) -> HotkeyGesture.Output
    func testHotkeyGestures() throws {
        let f: FixtureFile<HotkeyInput, [HotkeyOutput]> = try SharedFixtures.load("hotkey-gestures.json", minimumCases: 4)
        for c in f.cases {
            XCTAssertEqual(c.input.events.count, c.expected.count, "\(c.name): one expected output per event")
            var gesture = HotkeyGesture()
            let start = Date()
            for (i, event) in c.input.events.enumerated() where i < c.expected.count {
                let at = start.addingTimeInterval(event.at)
                let out: HotkeyGesture.Output
                switch event.key {
                case "down": out = gesture.keyDown(at: at)
                case "up": out = gesture.keyUp(at: at)
                default:
                    XCTFail("\(c.name): unknown key \(event.key)")
                    continue
                }
                let want = c.expected[i]
                let label: String
                var duration: Double?
                switch out {
                case .none: label = "none"
                case .pressBegan: label = "press_began"
                case .pressEnded(let d): label = "press_ended"; duration = d
                case .shortTap: label = "short_tap"
                case .doubleTap: label = "double_tap"
                }
                XCTAssertEqual(label, want.output, "\(c.name) event \(i)")
                if let d = want.duration {
                    XCTAssertEqual(duration ?? -1, d, accuracy: 0.001, "\(c.name) event \(i) duration")
                } else {
                    XCTAssertNil(duration, "\(c.name) event \(i) has no expected duration")
                }
            }
        }
    }

    // PassthroughFormatter.clean(_ text: String) -> String
    func testPassthroughClean() throws {
        let f: FixtureFile<TextInput, String> = try SharedFixtures.load("passthrough-clean.json", minimumCases: 5)
        for c in f.cases {
            XCTAssertEqual(PassthroughFormatter.clean(c.input.text), c.expected, c.name)
        }
    }

    // ModeResolver.wordCount(_ text: String) -> Int
    func testWordCount() throws {
        let f: FixtureFile<TextInput, Int> = try SharedFixtures.load("word-count.json", minimumCases: 4)
        for c in f.cases {
            XCTAssertEqual(ModeResolver.wordCount(c.input.text), c.expected, c.name)
        }
    }

    // ProfileResolver.resolve(_ ctx: TargetContext) -> Profile
    func testProfileResolution() throws {
        let f: FixtureFile<ProfileInput, String> = try SharedFixtures.load("profile-resolution.json", minimumCases: 60)
        for c in f.cases {
            let ctx = TargetContext(bundleID: c.input.bundleID, windowTitle: c.input.windowTitle, url: c.input.url)
            XCTAssertEqual(ProfileResolver.resolve(ctx).id, c.expected, c.name)
        }
    }

    // ModeResolver.resolve(requested: Mode, profile: Profile, wordCount: Int, promptModeMinWords: Int) -> Mode
    func testModeResolution() throws {
        let f: FixtureFile<ModeInput, String> = try SharedFixtures.load("mode-resolution.json", minimumCases: 5)
        for c in f.cases {
            let requested = try XCTUnwrap(Mode(rawValue: c.input.requested), "\(c.name): mode \(c.input.requested)")
            let profile = try XCTUnwrap(BuiltInProfiles.byID(c.input.profile), "\(c.name): profile \(c.input.profile)")
            let mode = ModeResolver.resolve(requested: requested, profile: profile, wordCount: c.input.wordCount,
                                            promptModeMinWords: c.input.promptModeMinWords)
            XCTAssertEqual(mode.rawValue, c.expected, c.name)
        }
    }

    // HallucinationFilter.isJunk(_ text: String) -> Bool
    func testHallucinationFilter() throws {
        let f: FixtureFile<TextInput, Bool> = try SharedFixtures.load("hallucination-filter.json", minimumCases: 25)
        for c in f.cases {
            XCTAssertEqual(HallucinationFilter.isJunk(c.input.text), c.expected, c.name)
        }
    }

    // WhisperKitEngine.retryLanguage(detected: String?, text: String) -> String?
    func testRetryLanguage() throws {
        let f: FixtureFile<RetryLanguageInput, String?> = try SharedFixtures.load("retry-language.json", minimumCases: 6)
        for c in f.cases {
            XCTAssertEqual(WhisperKitEngine.retryLanguage(detected: c.input.detected, text: c.input.text), c.expected, c.name)
        }
    }

    // LivePreview.join(_ a: String, _ b: String) -> String
    func testLivePreviewJoin() throws {
        let f: FixtureFile<JoinInput, String> = try SharedFixtures.load("live-preview-join.json", minimumCases: 3)
        for c in f.cases {
            XCTAssertEqual(LivePreview.join(c.input.a, c.input.b), c.expected, c.name)
        }
    }

    // AppVersion.isNewer(_ a: String, than b: String) -> Bool
    func testAppVersion() throws {
        let f: FixtureFile<VersionInput, Bool> = try SharedFixtures.load("app-version.json", minimumCases: 7)
        for c in f.cases {
            XCTAssertEqual(AppVersion.isNewer(c.input.candidate, than: c.input.installed), c.expected, c.name)
        }
    }

    // PromptBuilder.userMessage(_ r: FormatRequest) -> String
    func testPromptMessage() throws {
        let f: FixtureFile<PromptMessageInput, String> = try SharedFixtures.load("prompt-message.json", minimumCases: 12)
        for c in f.cases {
            let x = c.input.context
            let ctx = TargetContext(bundleID: x.bundleID, appName: x.app, windowTitle: x.windowTitle, url: x.url,
                                    textBeforeCaret: x.textBeforeCaret, selectedText: x.selectedText)
            let profile = try XCTUnwrap(BuiltInProfiles.byID(c.input.profile), "\(c.name): profile \(c.input.profile)")
            let mode = try XCTUnwrap(Mode(rawValue: c.input.mode), "\(c.name): mode \(c.input.mode)")
            let request = FormatRequest(transcript: c.input.transcript, languageHint: c.input.languageHint, mode: mode,
                                        profile: profile, context: ctx, dictionary: c.input.dictionary ?? [],
                                        examples: (c.input.examples ?? []).map { ($0.raw, $0.final) })
            XCTAssertEqual(PromptBuilder.userMessage(request), c.expected, c.name)
        }
    }

    // PromptBuilder.parseReply(_ text: String) -> (text, language, newTerms)?
    func testReplyParse() throws {
        let f: FixtureFile<ReplyInput, ParsedReplyExpected?> = try SharedFixtures.load("reply-parse.json", minimumCases: 14)
        for c in f.cases {
            let parsed = PromptBuilder.parseReply(c.input.reply)
            let got = parsed.map { ParsedReplyExpected(text: $0.text, language: $0.language, newTerms: $0.newTerms) }
            XCTAssertEqual(got, c.expected, c.name)
        }
    }

    // CodexFormatter.finish(_ raw: String, profile: Profile) -> String
    func testReplyFinish() throws {
        let f: FixtureFile<FinishInput, String> = try SharedFixtures.load("reply-finish.json", minimumCases: 9)
        for c in f.cases {
            let profile = try XCTUnwrap(BuiltInProfiles.byID(c.input.profile), "\(c.name): profile \(c.input.profile)")
            XCTAssertEqual(CodexFormatter.finish(c.input.text, profile: profile), c.expected, c.name)
        }
    }

    // FormattingService.format(_ request: FormatRequest) -> FormatResult, with a stub agent.
    // The cases with secure true check the rule "skip the agent when context.secureInput".
    func testPolishRouting() async throws {
        let f: FixtureFile<RoutingInput, RoutingExpected> = try SharedFixtures.load("polish-routing.json", minimumCases: 11)
        for c in f.cases {
            let claude: ShepitCore.Formatter?
            switch c.input.agent {
            case "none": claude = nil
            case "reply": claude = StubAgent(reply: c.input.agentText ?? "")
            case "error": claude = StubAgent(reply: nil)
            default: throw FixtureError(description: "\(c.name): agent \(c.input.agent)")
            }
            let service = FormattingService(claude: claude)
            await service.setFastPath(maxWords: c.input.fastPathMaxWords)
            let profile = try XCTUnwrap(BuiltInProfiles.byID(c.input.profile), "\(c.name): profile \(c.input.profile)")
            let mode = try XCTUnwrap(Mode(rawValue: c.input.mode), "\(c.name): mode \(c.input.mode)")
            let request = FormatRequest(transcript: c.input.transcript, languageHint: nil, mode: mode, profile: profile,
                                        context: TargetContext(secureInput: c.input.secure ?? false),
                                        dictionary: c.input.dictionary)
            let result = await service.format(request)
            XCTAssertEqual(RoutingExpected(source: result.source.rawValue, text: result.text), c.expected, c.name)
        }
    }

    func testEveryFixtureFileHasARunner() throws {
        let dir = SharedFixtures.fixturesDirectory
        let found = try FileManager.default.contentsOfDirectory(atPath: dir.path).filter { $0.hasSuffix(".json") }
        XCTAssertEqual(Set(found), Set(SharedFixtures.fileNames), "fixtures in \(dir.path)")
    }
}

/// Stands in for Claude or Codex: returns `reply` as the final text, or throws when `reply` is nil.
private struct StubAgent: ShepitCore.Formatter {
    let reply: String?

    func format(_ request: FormatRequest) async throws -> FormatResult {
        guard let reply else { throw WorkerError.timeout }
        return FormatResult(text: reply, source: .claude)
    }

    func warmUp() async {}
}
