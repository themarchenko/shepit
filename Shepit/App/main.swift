import AppKit

private var delegateHolder: AppDelegate?

MainActor.assumeIsolated {
    let app = NSApplication.shared
    let delegate = AppDelegate()
    delegateHolder = delegate
    app.delegate = delegate
    app.setActivationPolicy(.accessory)
    app.run()
}
