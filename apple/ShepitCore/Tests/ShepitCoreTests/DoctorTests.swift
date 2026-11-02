import AVFoundation
import XCTest
@testable import ShepitCore

/// All-green inputs by default; each test changes only what it is about.
/// The memberwise argument order of `DoctorInputs` must match this call.
private func inputs(
    accessibilityTrusted: Bool = true,
    accessibilityReset: AccessibilityResetState = .idle,
    microphone: MicrophoneAccess = .granted,
    microphoneReset: AccessibilityResetState = .idle,
    listenEventAccess: Bool = true,
    hotkey: HotkeyChoice = .rightOption,
    fnUsageType: Int? = 0,
    engine: EngineID = .parakeetV3,
    modelOnDisk: Bool = true,
    modelState: EngineLoadState = .loaded,
    polishing: PolishingProvider = .off,
    agentExecutable: String? = nil,
    agentTest: AgentTestState = .notRun,
    secureInputActive: Bool = false,
    secureInputHolder: String? = nil
) -> DoctorInputs {
    DoctorInputs(accessibilityTrusted: accessibilityTrusted, accessibilityReset: accessibilityReset,
                 microphone: microphone, microphoneReset: microphoneReset, listenEventAccess: listenEventAccess, hotkey: hotkey,
                 fnUsageType: fnUsageType, engine: engine, modelOnDisk: modelOnDisk, modelState: modelState,
                 polishing: polishing, agentExecutable: agentExecutable, agentTest: agentTest,
                 secureInputActive: secureInputActive, secureInputHolder: secureInputHolder)
}

private func row(_ id: DoctorCheckID, _ i: DoctorInputs, file: StaticString = #filePath, line: UInt = #line) -> DoctorCheck {
    guard let c = Doctor.evaluate(i).first(where: { $0.id == id }) else {
        XCTFail("no row for \(id)", file: file, line: line)
        return DoctorCheck(id: id, title: "", status: .ok, detail: "", fix: nil, fixTitle: nil, steps: [], required: false)
    }
    return c
}

final class DoctorTests: XCTestCase {
    // MARK: Accessibility

    func testAccessibilityTrustedIsOk() {
        let c = row(.accessibility, inputs(accessibilityTrusted: true))
        XCTAssertEqual(c.status, .ok)
        XCTAssertNil(c.fix)
    }

    func testAccessibilityUntrustedExplainsOlderBuildAndOffersReset() {
        let c = row(.accessibility, inputs(accessibilityTrusted: false))
        XCTAssertEqual(c.status, .problem)
        XCTAssertEqual(c.detail, "Not allowed. If Shepit is already switched on in the list, macOS is holding the permission of an older build.")
        XCTAssertEqual(c.fix, .resetAccessibility)
        XCTAssertEqual(c.fixTitle, "Reset and ask again")
        XCTAssertTrue(c.required)
    }

    func testAccessibilityAfterResetShowsSteps() {
        let c = row(.accessibility, inputs(accessibilityTrusted: false, accessibilityReset: .done))
        XCTAssertEqual(c.status, .problem)
        XCTAssertEqual(c.steps, ["Switch Shepit on in the list.", "Come back here. This row turns green by itself."])
    }

    func testAccessibilityResetFailureShowsManualSteps() {
        let c = row(.accessibility, inputs(accessibilityTrusted: false, accessibilityReset: .failed))
        XCTAssertEqual(c.steps, [
            "In the list, select Shepit and remove it with the minus button.",
            "Click Reset and ask again, then switch Shepit on.",
            "Come back here. This row turns green by itself.",
        ])
    }

    func testAccessibilityResetRunningIsChecking() {
        let c = row(.accessibility, inputs(accessibilityTrusted: false, accessibilityReset: .running))
        XCTAssertEqual(c.status, .checking)
    }

    // MARK: Microphone

    func testMicrophoneGranted() {
        let c = row(.microphone, inputs(microphone: .granted))
        XCTAssertEqual(c.status, .ok)
        XCTAssertTrue(c.required)
    }

    func testMicrophoneDeniedOffersResetAndAsk() {
        let c = row(.microphone, inputs(microphone: .denied))
        XCTAssertEqual(c.status, .problem)
        XCTAssertEqual(c.fix, .resetMicrophone)
        XCTAssertEqual(c.fixTitle, "Reset and ask again")
        XCTAssertEqual(c.detail, "Not allowed. If Shepit is already switched on in the list, macOS is holding the permission of an older build.")
        XCTAssertEqual(c.steps, ["Click Reset and ask again, then allow the microphone."])
        XCTAssertTrue(c.required)
    }

    func testMicrophoneResetRunningIsChecking() {
        let c = row(.microphone, inputs(microphone: .denied, microphoneReset: .running))
        XCTAssertEqual(c.status, .checking)
    }

    func testMicrophoneNotDeterminedRequests() {
        let c = row(.microphone, inputs(microphone: .notDetermined))
        XCTAssertEqual(c.status, .problem)
        XCTAssertEqual(c.fix, .requestMicrophone)
        XCTAssertEqual(c.fixTitle, "Allow")
    }

    // MARK: Input Monitoring

    func testInputMonitoringMissingIsOnlyInfo() {
        let c = row(.inputMonitoring, inputs(listenEventAccess: false))
        XCTAssertEqual(c.status, .info)
        XCTAssertEqual(c.steps, ["If Shepit is not in the list, click + and add it from Applications."])
        XCTAssertEqual(c.fix, .openInputMonitoring)
        XCTAssertFalse(c.required)
        XCTAssertEqual(row(.inputMonitoring, inputs(listenEventAccess: true)).status, .ok)
    }

    // MARK: Hotkey

    func testFnHotkeyWithFnActionIsAProblem() {
        for value in [1, 2, 3] {
            let c = row(.hotkey, inputs(hotkey: .fn, fnUsageType: value))
            XCTAssertEqual(c.status, .problem, "AppleFnUsageType \(value)")
            XCTAssertEqual(c.fix, .openKeyboardSettings)
            XCTAssertEqual(c.steps, [
                "In Keyboard settings set \"Press Fn key to\" to \"Do Nothing\".",
                "Come back here. This row turns green by itself.",
            ])
        }
    }

    func testFnHotkeyWithDoNothingIsOk() {
        XCTAssertEqual(row(.hotkey, inputs(hotkey: .fn, fnUsageType: 0)).status, .ok)
    }

    func testFnHotkeyWithUnreadableSettingIsWarning() {
        let c = row(.hotkey, inputs(hotkey: .fn, fnUsageType: nil))
        XCTAssertEqual(c.status, .warning)
        XCTAssertEqual(c.fix, .openKeyboardSettings)
    }

    func testOtherHotkeyIgnoresFnSetting() {
        let c = row(.hotkey, inputs(hotkey: .rightOption, fnUsageType: 2))
        XCTAssertEqual(c.status, .ok)
        XCTAssertEqual(c.detail, "Uses Right ⌥. The Fn setting does not matter.")
    }

    // MARK: Model

    func testModelLoaded() {
        XCTAssertEqual(row(.model, inputs(modelState: .loaded)).status, .ok)
    }

    func testModelLoadingIsChecking() {
        let c = row(.model, inputs(engine: .parakeetV3, modelState: .loading))
        XCTAssertEqual(c.status, .checking)
        XCTAssertEqual(c.detail, "Loading \(EngineID.parakeetV3.displayName). The first time it downloads 0.6-1.6 GB.")
    }

    func testModelFailedOffersDownload() {
        let long = "first line " + String(repeating: "x", count: 300) + "\nsecond line"
        let c = row(.model, inputs(modelState: .failed(long)))
        XCTAssertEqual(c.status, .problem)
        XCTAssertEqual(c.fix, .downloadModel)
        XCTAssertEqual(c.fixTitle, "Download now")
        XCTAssertTrue(c.detail.hasPrefix("Could not load: first line"))
        XCTAssertFalse(c.detail.contains("second line"))
        XCTAssertLessThanOrEqual(c.detail.count, "Could not load: ".count + 120)
    }

    func testModelMissingOffersDownload() {
        let c = row(.model, inputs(modelOnDisk: false, modelState: .idle))
        XCTAssertEqual(c.status, .warning)
        XCTAssertEqual(c.detail, "Not downloaded yet.")
        XCTAssertEqual(c.fix, .downloadModel)
    }

    func testModelOnDiskNotLoadedIsOk() {
        XCTAssertEqual(row(.model, inputs(modelOnDisk: true, modelState: .idle)).status, .ok)
    }

    // MARK: Agent

    func testAgentRowHiddenWhenPolishingOff() {
        let rows = Doctor.evaluate(inputs(polishing: .off))
        XCTAssertFalse(rows.contains { $0.id == .agent })
    }

    func testAgentMissingExecutableIsProblem() {
        let c = row(.agent, inputs(polishing: .claudeCode, agentExecutable: nil))
        XCTAssertEqual(c.status, .problem)
        XCTAssertEqual(c.fix, .openPolishingSettings)
    }

    func testAgentFoundNotTestedIsOkWithTest() {
        let c = row(.agent, inputs(polishing: .claudeCode, agentExecutable: "/usr/local/bin/claude", agentTest: .notRun))
        XCTAssertEqual(c.status, .ok)
        XCTAssertEqual(c.detail, "Found at /usr/local/bin/claude.")
        XCTAssertEqual(c.fix, .testAgent)
        XCTAssertEqual(c.fixTitle, "Test")
        XCTAssertEqual(row(.agent, inputs(polishing: .claudeCode, agentExecutable: "/x/claude", agentTest: .running)).status, .checking)
        XCTAssertEqual(row(.agent, inputs(polishing: .claudeCode, agentExecutable: "/x/claude", agentTest: .passed)).status, .ok)
    }

    func testAgentTestFailureMapsKindToPlainText() {
        let kinds = ["timeout", "exited(1)", "apiError", "paused", "badReply", "unknown"]
        var seen = Set<String>()
        for kind in kinds {
            let text = Doctor.agentFailureText(kind: kind)
            XCTAssertFalse(text.isEmpty, kind)
            XCTAssertFalse(text.contains("exited("), "raw kind leaked for \(kind)")
            seen.insert(text)
            let c = row(.agent, inputs(polishing: .claudeCode, agentExecutable: "/x/claude", agentTest: .failed(kind: kind)))
            XCTAssertEqual(c.status, .problem, kind)
            XCTAssertEqual(c.detail, text, kind)
        }
        XCTAssertEqual(seen.count, kinds.count, "each kind gets its own text")
    }

    // MARK: Secure input

    func testSecureInputNamesHolder() {
        let c = row(.secureInput, inputs(secureInputActive: true, secureInputHolder: "1Password"))
        XCTAssertEqual(c.status, .warning)
        XCTAssertEqual(c.detail, "1Password has secure input on, so Shepit cannot paste. Close or leave its password field.")
        XCTAssertNil(c.fix)
    }

    func testSecureInputWithoutHolderSaysAnotherApp() {
        let c = row(.secureInput, inputs(secureInputActive: true, secureInputHolder: nil))
        XCTAssertEqual(c.detail, "Another app has secure input on, so Shepit cannot paste. Close or leave its password field.")
        XCTAssertEqual(row(.secureInput, inputs(secureInputActive: false)).status, .ok)
    }

    // MARK: Aggregates

    func testRequiredProblemCountsOnlyAccessibilityAndMicrophone() {
        XCTAssertFalse(Doctor.requiredProblem(accessibilityTrusted: true, microphone: .granted))
        XCTAssertTrue(Doctor.requiredProblem(accessibilityTrusted: false, microphone: .granted))
        XCTAssertTrue(Doctor.requiredProblem(accessibilityTrusted: true, microphone: .denied))
        XCTAssertTrue(Doctor.requiredProblem(accessibilityTrusted: true, microphone: .notDetermined))
        let rows = Doctor.evaluate(inputs(listenEventAccess: false, modelOnDisk: false, modelState: .failed("x"), secureInputActive: true))
        XCTAssertEqual(rows.filter(\.required).map(\.id), [.accessibility, .microphone])
    }

    func testAutoOpen() {
        func open(onboarding: Bool = true, tour: Bool = false, already: Bool = false,
                  ax: Bool = true, mic: MicrophoneAccess = .granted) -> Bool {
            Doctor.shouldAutoOpen(onboardingCompleted: onboarding, tourOpen: tour, alreadyOpened: already,
                                  accessibilityTrusted: ax, microphone: mic)
        }
        XCTAssertFalse(open(onboarding: false, ax: false))
        XCTAssertFalse(open(tour: true, ax: false))
        XCTAssertFalse(open(already: true, ax: false))
        XCTAssertFalse(open())
        XCTAssertTrue(open(ax: false))
        XCTAssertTrue(open(mic: .denied))
        XCTAssertTrue(open(mic: .notDetermined))
    }

    func testSummaryCountsNonOkRows() {
        XCTAssertEqual(Doctor.summary(Doctor.evaluate(inputs())), "Everything Shepit needs is working.")
        let two = Doctor.evaluate(inputs(accessibilityTrusted: false, listenEventAccess: false))
        XCTAssertEqual(Doctor.summary(two), "1 things need attention.")
    }

    func testInputMonitoringOffAloneDoesNotCount() {
        let rows = Doctor.evaluate(inputs(listenEventAccess: false))
        XCTAssertEqual(Doctor.summary(rows), "Everything Shepit needs is working.")
        let two = Doctor.evaluate(inputs(accessibilityTrusted: false, listenEventAccess: false, modelOnDisk: false, modelState: .idle))
        XCTAssertEqual(Doctor.summary(two), "2 things need attention.")
    }

    func testCheckOrder() {
        let all = Doctor.evaluate(inputs(polishing: .claudeCode, agentExecutable: "/x/claude")).map(\.id)
        XCTAssertEqual(all, [.accessibility, .microphone, .hotkey, .model, .agent, .secureInput, .inputMonitoring])
    }

    // MARK: Pure parsers and mappings

    @MainActor
    func testFnUsageTypeParsing() {
        XCTAssertNil(SystemProbe.parseFnUsageType(nil))
        XCTAssertEqual(SystemProbe.parseFnUsageType(NSNumber(value: 0)), 0)
        XCTAssertEqual(SystemProbe.parseFnUsageType(NSNumber(value: 2)), 2)
        XCTAssertNil(SystemProbe.parseFnUsageType("2"))
    }

    @MainActor
    func testSecureInputPIDParsing() {
        XCTAssertNil(SystemProbe.secureInputPID(in: nil))
        XCTAssertNil(SystemProbe.secureInputPID(in: ["other": 5]))
        XCTAssertNil(SystemProbe.secureInputPID(in: ["kCGSSessionSecureInputPID": 0]))
        XCTAssertEqual(SystemProbe.secureInputPID(in: ["kCGSSessionSecureInputPID": 123]), 123)
    }

    func testMicrophoneAccessFromAuthorizationStatus() {
        XCTAssertEqual(MicrophoneAccess(status: .authorized), .granted)
        XCTAssertEqual(MicrophoneAccess(status: .denied), .denied)
        XCTAssertEqual(MicrophoneAccess(status: .restricted), .denied)
        XCTAssertEqual(MicrophoneAccess(status: .notDetermined), .notDetermined)
    }
}
