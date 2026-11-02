import Foundation

public enum DoctorStatus: Equatable, Sendable { case ok, warning, problem, checking
    /// Not a fault: optional, shown in gray, never counted in the summary.
    case info
}

public enum DoctorCheckID: Equatable, Sendable {
    case accessibility, microphone, hotkey, model, agent, secureInput, inputMonitoring
}

public enum DoctorFix: Equatable, Sendable {
    case resetAccessibility, resetMicrophone, requestMicrophone, openKeyboardSettings
    case downloadModel, openPolishingSettings, testAgent, openInputMonitoring
}

public enum MicrophoneAccess: Equatable, Sendable { case granted, denied, notDetermined }

public enum AccessibilityResetState: Equatable, Sendable { case idle, running, done, failed }

public enum AgentTestState: Equatable, Sendable {
    case notRun, running, passed
    case failed(kind: String)
}

public struct DoctorCheck: Equatable, Sendable, Identifiable {
    public var id: DoctorCheckID
    public var title: String
    public var status: DoctorStatus
    public var detail: String
    public var fix: DoctorFix?
    public var fixTitle: String?
    public var steps: [String]
    public var required: Bool

    public init(id: DoctorCheckID, title: String, status: DoctorStatus, detail: String, fix: DoctorFix?,
                fixTitle: String?, steps: [String], required: Bool) {
        self.id = id
        self.title = title
        self.status = status
        self.detail = detail
        self.fix = fix
        self.fixTitle = fixTitle
        self.steps = steps
        self.required = required
    }
}

public struct DoctorInputs: Equatable, Sendable {
    public var accessibilityTrusted: Bool
    public var accessibilityReset: AccessibilityResetState
    public var microphone: MicrophoneAccess
    public var microphoneReset: AccessibilityResetState
    public var listenEventAccess: Bool
    public var hotkey: HotkeyChoice
    public var fnUsageType: Int?
    public var engine: EngineID
    public var modelOnDisk: Bool
    public var modelState: EngineLoadState
    public var polishing: PolishingProvider
    public var agentExecutable: String?
    public var agentTest: AgentTestState
    public var secureInputActive: Bool
    public var secureInputHolder: String?

    public init(accessibilityTrusted: Bool, accessibilityReset: AccessibilityResetState, microphone: MicrophoneAccess,
                microphoneReset: AccessibilityResetState = .idle, listenEventAccess: Bool, hotkey: HotkeyChoice, fnUsageType: Int?, engine: EngineID,
                modelOnDisk: Bool, modelState: EngineLoadState, polishing: PolishingProvider,
                agentExecutable: String?, agentTest: AgentTestState, secureInputActive: Bool,
                secureInputHolder: String?) {
        self.accessibilityTrusted = accessibilityTrusted
        self.accessibilityReset = accessibilityReset
        self.microphone = microphone
        self.microphoneReset = microphoneReset
        self.listenEventAccess = listenEventAccess
        self.hotkey = hotkey
        self.fnUsageType = fnUsageType
        self.engine = engine
        self.modelOnDisk = modelOnDisk
        self.modelState = modelState
        self.polishing = polishing
        self.agentExecutable = agentExecutable
        self.agentTest = agentTest
        self.secureInputActive = secureInputActive
        self.secureInputHolder = secureInputHolder
    }
}

/// Pure evaluation of what keeps Shepit from working. The probes live elsewhere.
public enum Doctor {
    private static var comeBack: String { L("Come back here. This row turns green by itself.") }

    public static func evaluate(_ i: DoctorInputs) -> [DoctorCheck] {
        var rows = [accessibility(i), microphone(i), hotkey(i), model(i)]
        if let agent = agent(i) { rows.append(agent) }
        rows.append(secureInput(i))
        rows.append(inputMonitoring(i))
        return rows
    }

    /// Accessibility and Microphone are the two without which dictation cannot work.
    public static func requiredProblem(accessibilityTrusted: Bool, microphone: MicrophoneAccess) -> Bool {
        !(accessibilityTrusted && microphone == .granted)
    }

    public static func shouldAutoOpen(onboardingCompleted: Bool, tourOpen: Bool, alreadyOpened: Bool,
                                      accessibilityTrusted: Bool, microphone: MicrophoneAccess) -> Bool {
        onboardingCompleted && !tourOpen && !alreadyOpened
            && requiredProblem(accessibilityTrusted: accessibilityTrusted, microphone: microphone)
    }

    public static func summary(_ checks: [DoctorCheck]) -> String {
        let n = checks.filter { $0.status != .ok && $0.status != .info }.count
        if n == 0 { return L("Everything Shepit needs is working.") }
        return L("\(n) things need attention.")
    }

    public static func agentFailureText(kind: String) -> String {
        switch kind {
        case "timeout": return L("The agent did not answer in time. Try again in a moment.")
        case "apiError": return L("The agent reported an error. It may be signed out or out of quota.")
        case "paused": return L("The agent is paused after repeated failures. Try again later.")
        case "badReply": return L("The agent answered with something Shepit could not read.")
        case "notRunning": return L("The agent is not running.")
        default:
            if kind.hasPrefix("exited(") { return L("The agent stopped unexpectedly. Check that it is installed and signed in.") }
            return L("The agent test failed for an unknown reason.")
        }
    }

    private static func accessibility(_ i: DoctorInputs) -> DoctorCheck {
        var c = DoctorCheck(id: .accessibility, title: L("Accessibility"), status: .ok,
                            detail: L("Allowed. Shepit can paste into other apps."), fix: nil, fixTitle: nil, steps: [], required: true)
        guard !i.accessibilityTrusted else { return c }
        c.status = .problem
        c.detail = L("Not allowed. If Shepit is already switched on in the list, macOS is holding the permission of an older build.")
        c.fix = .resetAccessibility
        c.fixTitle = L("Reset and ask again")
        switch i.accessibilityReset {
        case .idle: break
        case .running: c.status = .checking
        case .done: c.steps = [L("Switch Shepit on in the list."), comeBack]
        case .failed:
            c.steps = [L("In the list, select Shepit and remove it with the minus button."),
                       L("Click Reset and ask again, then switch Shepit on."), comeBack]
        }
        return c
    }

    private static func microphone(_ i: DoctorInputs) -> DoctorCheck {
        var c = DoctorCheck(id: .microphone, title: L("Microphone"), status: .ok,
                            detail: L("Allowed."), fix: nil, fixTitle: nil, steps: [], required: true)
        switch i.microphone {
        case .granted: break
        case .denied:
            c.status = .problem
            // Same stale-grant trap as Accessibility: a grant made to an older build can sit switched on.
            c.detail = L("Not allowed. If Shepit is already switched on in the list, macOS is holding the permission of an older build.")
            c.fix = .resetMicrophone
            c.fixTitle = L("Reset and ask again")
            c.steps = [L("Click Reset and ask again, then allow the microphone.")]
            switch i.microphoneReset {
            case .running: c.status = .checking
            case .failed:
                c.steps = [L("In the list, select Shepit and remove it with the minus button."),
                           L("Click Reset and ask again, then allow the microphone.")]
            case .idle, .done: break
            }
        case .notDetermined:
            c.status = .problem
            c.detail = L("Shepit has not asked for the microphone yet.")
            c.fix = .requestMicrophone
            c.fixTitle = L("Allow")
        }
        return c
    }

    private static func hotkey(_ i: DoctorInputs) -> DoctorCheck {
        var c = DoctorCheck(id: .hotkey, title: L("Dictation key"), status: .ok,
                            detail: L("Uses \(i.hotkey.keyLabel). The Fn setting does not matter."), fix: nil, fixTitle: nil,
                            steps: [], required: false)
        guard i.hotkey == .fn else { return c }
        switch i.fnUsageType {
        case 0:
            c.detail = L("Uses Fn. The system leaves it to Shepit.")
            return c
        case .some:
            c.status = .problem
            c.detail = L("The system also reacts to Fn (emoji picker, dictation or input source).")
        case .none:
            c.status = .warning
            c.detail = L("Could not read the Fn setting. If Fn does nothing in Shepit, check Keyboard settings.")
        }
        c.fix = .openKeyboardSettings
        c.fixTitle = L("Open Keyboard settings")
        if c.status == .problem {
            c.steps = [L("In Keyboard settings set \"Press Fn key to\" to \"Do Nothing\"."), comeBack]
        }
        return c
    }

    private static func model(_ i: DoctorInputs) -> DoctorCheck {
        var c = DoctorCheck(id: .model, title: L("Speech model"), status: .ok,
                            detail: L("\(i.engine.displayName) is ready."), fix: nil, fixTitle: nil, steps: [], required: false)
        switch i.modelState {
        case .loaded: break
        case .loading:
            c.status = .checking
            c.detail = L("Loading \(i.engine.displayName). The first time it downloads 0.6-1.6 GB.")
        case .failed(let message):
            let first = message.split(separator: "\n", omittingEmptySubsequences: true).first.map(String.init) ?? message
            c.status = .problem
            c.detail = L("Could not load: \(String(first.prefix(120)))")
            c.fix = .downloadModel
            c.fixTitle = L("Download now")
        case .idle:
            if !i.modelOnDisk {
                c.status = .warning
                c.detail = L("Not downloaded yet.")
                c.fix = .downloadModel
                c.fixTitle = L("Download now")
            } else {
                c.detail = L("\(i.engine.displayName) is downloaded and loads when needed.")
            }
        }
        return c
    }

    private static func agent(_ i: DoctorInputs) -> DoctorCheck? {
        guard i.polishing != .off else { return nil }
        var c = DoctorCheck(id: .agent, title: i.polishing.displayName, status: .ok, detail: "", fix: nil,
                            fixTitle: nil, steps: [], required: false)
        guard let path = i.agentExecutable else {
            c.status = .problem
            c.detail = L("\(i.polishing.displayName) was not found on this Mac.")
            c.fix = .openPolishingSettings
            c.fixTitle = L("Open Polishing settings")
            return c
        }
        c.detail = L("Found at \(path).")
        c.fix = .testAgent
        c.fixTitle = L("Test")
        switch i.agentTest {
        case .notRun: break
        case .running: c.status = .checking
        case .passed: c.detail = L("Found at \(path). The test passed.")
        case .failed(let kind):
            c.status = .problem
            c.detail = agentFailureText(kind: kind)
        }
        return c
    }

    private static func secureInput(_ i: DoctorInputs) -> DoctorCheck {
        var c = DoctorCheck(id: .secureInput, title: L("Secure input"), status: .ok,
                            detail: L("No app is blocking pasting."), fix: nil, fixTitle: nil, steps: [], required: false)
        if i.secureInputActive {
            c.status = .warning
            let holder = i.secureInputHolder ?? L("Another app")
            c.detail = L("\(holder) has secure input on, so Shepit cannot paste. Close or leave its password field.")
        }
        return c
    }

    private static func inputMonitoring(_ i: DoctorInputs) -> DoctorCheck {
        var c = DoctorCheck(id: .inputMonitoring, title: L("Input Monitoring"), status: .ok,
                            detail: L("Allowed."), fix: nil, fixTitle: nil, steps: [], required: false)
        if !i.listenEventAccess {
            c.status = .info
            c.detail = L("Shepit usually works without it. Allow it only if the dictation key does not respond.")
            c.steps = [L("If Shepit is not in the list, click + and add it from Applications.")]
            c.fix = .openInputMonitoring
            c.fixTitle = L("Open Input Monitoring")
        }
        return c
    }
}
