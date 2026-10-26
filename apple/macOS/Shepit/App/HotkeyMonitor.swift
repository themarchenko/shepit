import AppKit
import ShepitCore

/// Watches the dictation modifier key system-wide. Needs Accessibility (and possibly Input Monitoring) to see events in other apps.
@MainActor
final class HotkeyMonitor {
    var onPressBegan: @MainActor () -> Void = {}
    var onPressEnded: @MainActor () -> Void = {}
    var onShortTap: @MainActor () -> Void = {}
    var onDoubleTap: @MainActor () -> Void = {}
    var onEscape: @MainActor () -> Void = {}

    private var choice: HotkeyChoice
    private var gesture = HotkeyGesture()
    private var globalFlags: Any?
    private var localFlags: Any?
    private var globalKeys: Any?
    private var localKeys: Any?

    init(choice: HotkeyChoice) {
        self.choice = choice
    }

    func update(choice: HotkeyChoice) {
        self.choice = choice
        gesture = HotkeyGesture()
    }

    func start() {
        guard globalFlags == nil else { return }
        globalFlags = NSEvent.addGlobalMonitorForEvents(matching: .flagsChanged) { [weak self] event in
            MainActor.assumeIsolated { self?.handleFlags(event) }
        }
        localFlags = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { [weak self] event in
            MainActor.assumeIsolated { self?.handleFlags(event) }
            return event
        }
        globalKeys = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { [weak self] event in
            MainActor.assumeIsolated { self?.handleKey(event) }
        }
        localKeys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            MainActor.assumeIsolated { self?.handleKey(event) }
            return event
        }
        Log.ui.notice("hotkey monitor started (\(self.choice.rawValue, privacy: .public))")
    }

    func stop() {
        for m in [globalFlags, localFlags, globalKeys, localKeys] {
            if let m { NSEvent.removeMonitor(m) }
        }
        globalFlags = nil
        localFlags = nil
        globalKeys = nil
        localKeys = nil
    }

    private var keySpec: (code: UInt16, flag: NSEvent.ModifierFlags) {
        switch choice {
        case .rightOption: return (61, .option)
        case .rightCommand: return (54, .command)
        case .fn: return (63, .function)
        }
    }

    private func handleFlags(_ event: NSEvent) {
        let spec = keySpec
        guard event.keyCode == spec.code else { return }
        let isDown = event.modifierFlags.contains(spec.flag)
        let output = isDown ? gesture.keyDown() : gesture.keyUp()
        if case .none = output {} else {
            Log.ui.notice("hotkey \(String(describing: output), privacy: .public)")
        }
        switch output {
        case .none: break
        case .pressBegan: onPressBegan()
        case .pressEnded: onPressEnded()
        case .shortTap: onShortTap()
        case .doubleTap: onDoubleTap()
        }
    }

    private func handleKey(_ event: NSEvent) {
        if event.keyCode == 53 { onEscape() }
    }
}
