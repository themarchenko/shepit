import XCTest
@testable import ShepitCore

/// Every user-facing string that ShepitCore produces has a Ukrainian version in the package catalog.
/// `CoreLocalization.override` is the test-only switch; production reads the main bundle's language.
final class CoreLocalizationTests: XCTestCase {
    override func setUpWithError() throws {
        try super.setUpWithError()
        try CoreLocalizationFixture.install()
    }

    override func tearDown() {
        CoreLocalizationFixture.reset()
        super.tearDown()
    }

    private func hasCyrillic(_ s: String) -> Bool {
        s.unicodeScalars.contains { (0x0400...0x04FF).contains($0.value) }
    }

    // MARK: Matrix inputs

    private func inputs(
        accessibilityTrusted: Bool = true, accessibilityReset: AccessibilityResetState = .idle,
        microphone: MicrophoneAccess = .granted, listenEventAccess: Bool = true,
        hotkey: HotkeyChoice = .rightOption, fnUsageType: Int? = 0, engine: EngineID = .parakeetV3,
        modelOnDisk: Bool = true, modelState: EngineLoadState = .loaded, polishing: PolishingProvider = .off,
        agentExecutable: String? = nil, agentTest: AgentTestState = .notRun,
        secureInputActive: Bool = false, secureInputHolder: String? = nil
    ) -> DoctorInputs {
        DoctorInputs(accessibilityTrusted: accessibilityTrusted, accessibilityReset: accessibilityReset,
                     microphone: microphone, listenEventAccess: listenEventAccess, hotkey: hotkey,
                     fnUsageType: fnUsageType, engine: engine, modelOnDisk: modelOnDisk, modelState: modelState,
                     polishing: polishing, agentExecutable: agentExecutable, agentTest: agentTest,
                     secureInputActive: secureInputActive, secureInputHolder: secureInputHolder)
    }

    /// Every branch of `Doctor.evaluate`, one scenario each.
    private var doctorScenarios: [(String, DoctorInputs)] {
        var list: [(String, DoctorInputs)] = [
            ("allOk", inputs(polishing: .claudeCode, agentExecutable: "/bin/claude", agentTest: .passed)),
            ("accessibility.idle", inputs(accessibilityTrusted: false)),
            ("accessibility.running", inputs(accessibilityTrusted: false, accessibilityReset: .running)),
            ("accessibility.done", inputs(accessibilityTrusted: false, accessibilityReset: .done)),
            ("accessibility.failed", inputs(accessibilityTrusted: false, accessibilityReset: .failed)),
            ("microphone.denied", inputs(microphone: .denied)),
            ("microphone.notDetermined", inputs(microphone: .notDetermined)),
            ("hotkey.fnOk", inputs(hotkey: .fn, fnUsageType: 0)),
            ("hotkey.fnProblem", inputs(hotkey: .fn, fnUsageType: 2)),
            ("hotkey.fnUnknown", inputs(hotkey: .fn, fnUsageType: nil)),
            ("model.loading", inputs(modelState: .loading)),
            ("model.failed", inputs(modelState: .failed("boom\nsecond line"))),
            ("model.notOnDisk", inputs(modelOnDisk: false, modelState: .idle)),
            ("model.onDisk", inputs(modelOnDisk: true, modelState: .idle)),
            ("agent.missing", inputs(polishing: .codex, agentExecutable: nil)),
            ("agent.notRun", inputs(polishing: .claudeCode, agentExecutable: "/bin/claude")),
            ("agent.running", inputs(polishing: .claudeCode, agentExecutable: "/bin/claude", agentTest: .running)),
            ("agent.passed", inputs(polishing: .codex, agentExecutable: "/bin/codex", agentTest: .passed)),
            ("secure.holder", inputs(secureInputActive: true, secureInputHolder: "Terminal")),
            ("secure.noHolder", inputs(secureInputActive: true, secureInputHolder: nil)),
            ("inputMonitoring.off", inputs(listenEventAccess: false)),
        ]
        for kind in agentKinds {
            list.append(("agent.failed.\(kind)", inputs(polishing: .claudeCode, agentExecutable: "/bin/claude",
                                                         agentTest: .failed(kind: kind))))
        }
        return list
    }

    private let agentKinds = ["timeout", "apiError", "paused", "badReply", "notRunning", "exited(1)", "mystery"]

    /// Keys of strings that must change in Ukrainian.
    private func translatable() -> [String: String] {
        var out: [String: String] = [:]
        for (name, scenario) in doctorScenarios {
            for row in Doctor.evaluate(scenario) {
                let base = "doctor.\(name).\(row.id)"
                if row.id != .agent { out["\(base).title"] = row.title }
                out["\(base).detail"] = row.detail
                if let fix = row.fixTitle { out["\(base).fixTitle"] = fix }
                for (n, step) in row.steps.enumerated() { out["\(base).step\(n)"] = step }
            }
        }
        for kind in agentKinds { out["doctor.failureText.\(kind)"] = Doctor.agentFailureText(kind: kind) }
        let ok = Doctor.evaluate(inputs())
        out["doctor.summary.0"] = Doctor.summary(ok)
        for n in [1, 2, 5, 21] { out["doctor.summary.\(n)"] = Doctor.summary(problems(n)) }

        for h in HotkeyChoice.allCases {
            out["hotkey.displayName.\(h)"] = h.displayName
            if h != .fn { out["hotkey.shortName.\(h)"] = h.shortName }
        }
        out["polishing.off"] = PolishingProvider.off.displayName
        for s in WidgetStyle.allCases {
            out["widget.displayName.\(s)"] = s.displayName
            out["widget.blurb.\(s)"] = s.blurb
        }
        for p in LanguagePolicy.allCases { out["language.\(p)"] = p.displayName }
        for m in Mode.allCases { out["mode.\(m)"] = m.displayName }

        out["insert.secureInput"] = InsertError.secureInput.errorDescription
        out["insert.empty"] = InsertError.empty.errorDescription
        out["audio.noInput"] = AudioError.noInput.errorDescription
        out["audio.denied"] = AudioError.permissionDenied.errorDescription
        out["audio.format"] = AudioError.formatMismatch(tap: 44100, hardware: 48000).errorDescription
        out["speech.notLoaded"] = SpeechError.notLoaded.errorDescription
        out["speech.loadFailed"] = SpeechError.loadFailed("disk").errorDescription
        out["speech.transcriptionFailed"] = SpeechError.transcriptionFailed("disk").errorDescription
        out["install.translocated"] = UpdateInstaller.InstallError.translocated.errorDescription
        out["install.notAnUpdate"] = UpdateInstaller.InstallError.notAnUpdate("0.4.0").errorDescription
        out["install.signature"] = UpdateInstaller.InstallError.signatureMismatch.errorDescription
        out["install.cancelled"] = UpdateInstaller.InstallError.cancelled.errorDescription
        out["install.tool"] = UpdateInstaller.InstallError.tool("hdiutil", "busy").errorDescription
        out["feed.rateLimit403"] = ReleaseFeed.FeedError.http(403).errorDescription
        out["feed.rateLimit429"] = ReleaseFeed.FeedError.http(429).errorDescription
        out["feed.http"] = ReleaseFeed.FeedError.http(500).errorDescription
        out["feed.noDiskImage"] = ReleaseFeed.FeedError.noDiskImage("0.6.0").errorDescription
        out["feed.insecure"] = ReleaseFeed.FeedError.insecureDiskImage("0.6.0").errorDescription
        return out
    }

    private func problems(_ n: Int) -> [DoctorCheck] {
        (0..<n).map { _ in
            DoctorCheck(id: .microphone, title: "", status: .problem, detail: "", fix: nil, fixTitle: nil,
                        steps: [], required: false)
        }
    }

    // MARK: Language selection

    func testEnglishWithoutOverride() {
        CoreLocalization.override = nil
        XCTAssertEqual(CoreLocalization.language, "en")
    }

    func testOverrideWins() {
        CoreLocalization.override = "uk"
        XCTAssertEqual(CoreLocalization.language, "uk")
    }

    // MARK: Ukrainian matrix

    func testEveryCoreStringIsTranslated() {
        CoreLocalization.override = "en"
        let english = translatable()
        CoreLocalization.override = "uk"
        let ukrainian = translatable()
        XCTAssertEqual(Set(english.keys), Set(ukrainian.keys))
        XCTAssertGreaterThan(english.count, 60)
        for (key, en) in english.sorted(by: { $0.key < $1.key }) {
            let uk = ukrainian[key] ?? ""
            XCTAssertNotEqual(uk, en, "\(key) is not translated")
            XCTAssertTrue(hasCyrillic(uk), "\(key) has no Cyrillic: \(uk)")
        }
    }

    func testEnglishMatrixHasNoCyrillic() {
        CoreLocalization.override = "en"
        for (key, value) in translatable() { XCTAssertFalse(hasCyrillic(value), key) }
    }

    func testProductAndEngineNamesStayLatin() {
        CoreLocalization.override = "uk"
        for e in EngineID.allCases { XCTAssertFalse(hasCyrillic(e.displayName), e.rawValue) }
        for m in ClaudeModel.allCases { XCTAssertFalse(hasCyrillic(m.displayName), m.rawValue) }
        XCTAssertEqual(PolishingProvider.claudeCode.displayName, "Claude Code")
        XCTAssertEqual(PolishingProvider.codex.displayName, "Codex")
    }

    func testUkrainianDetailsKeepTheirValues() {
        CoreLocalization.override = "uk"
        let model = Doctor.evaluate(inputs(modelState: .failed("boom\nsecond"))).first { $0.id == .model }
        XCTAssertTrue(model?.detail.contains("boom") ?? false)
        XCTAssertFalse(model?.detail.contains("second") ?? true)
        let agent = Doctor.evaluate(inputs(polishing: .codex, agentExecutable: "/opt/codex")).first { $0.id == .agent }
        XCTAssertTrue(agent?.detail.contains("/opt/codex") ?? false)
        let secure = Doctor.evaluate(inputs(secureInputActive: true, secureInputHolder: "Terminal")).first { $0.id == .secureInput }
        XCTAssertTrue(secure?.detail.contains("Terminal") ?? false)
        XCTAssertTrue(AudioError.formatMismatch(tap: 44100, hardware: 48000).errorDescription?.contains("48000") ?? false)
        XCTAssertTrue(ReleaseFeed.FeedError.http(500).errorDescription?.contains("500") ?? false)
        XCTAssertTrue(ReleaseFeed.FeedError.noDiskImage("0.6.0").errorDescription?.contains("0.6.0") ?? false)
    }

    // MARK: Plurals

    func testEnglishSummaryPlurals() {
        CoreLocalization.override = "en"
        XCTAssertEqual(Doctor.summary(problems(1)), "1 thing needs attention.")
        XCTAssertEqual(Doctor.summary(problems(2)), "2 things need attention.")
        XCTAssertEqual(Doctor.summary(problems(21)), "21 things need attention.")
    }

    func testUkrainianSummaryUsesThreeFormsWithTheCount() {
        CoreLocalization.override = "uk"
        let one = Doctor.summary(problems(1)), few = Doctor.summary(problems(2)), many = Doctor.summary(problems(5))
        XCTAssertEqual(Set([one, few, many]).count, 3)
        XCTAssertTrue(one.contains("1"))
        XCTAssertTrue(few.contains("2"))
        XCTAssertTrue(many.contains("5"))
    }

    func testUkrainianTwentyOneTakesTheSingularForm() {
        CoreLocalization.override = "uk"
        let one = Doctor.summary(problems(1))
        let twentyOne = Doctor.summary(problems(21))
        XCTAssertEqual(twentyOne.replacingOccurrences(of: "21", with: "1"), one)
    }

    func testUkrainianAllWorkingSummary() {
        CoreLocalization.override = "uk"
        let s = Doctor.summary(Doctor.evaluate(inputs()))
        XCTAssertTrue(hasCyrillic(s))
    }
}
