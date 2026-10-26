import AppKit

/// Borderless, non-activating panel that floats above other windows on every Space and never takes keyboard focus.
final class WidgetPanel: NSPanel {
    init(contentRect: NSRect) {
        super.init(contentRect: contentRect, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .statusBar
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false // the SwiftUI glass draws its own shadow, which follows the animated shape
        hidesOnDeactivate = false
        isMovableByWindowBackground = false
        isReleasedWhenClosed = false
        becomesKeyOnlyIfNeeded = true
        animationBehavior = .utilityWindow
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// Mouse handling for the widget: click, press-and-hold, drag, right click, hover.
final class WidgetHostView: NSView {
    var onClick: @MainActor () -> Void = {}
    var onHoldBegan: @MainActor () -> Void = {}
    var onHoldEnded: @MainActor () -> Void = {}
    var onDragEnded: @MainActor () -> Void = {}
    var onRightClick: @MainActor (NSEvent) -> Void = { _ in }
    var onHover: @MainActor (Bool) -> Void = { _ in }

    private var lastScreenPoint: NSPoint?
    private var dragging = false
    private var holding = false
    private var holdTimer: Timer?
    private var trackingArea: NSTrackingArea?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea { removeTrackingArea(trackingArea) }
        let area = NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self)
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseEntered(with event: NSEvent) { onHover(true) }
    override func mouseExited(with event: NSEvent) { onHover(false) }

    override func mouseDown(with event: NSEvent) {
        lastScreenPoint = NSEvent.mouseLocation
        dragging = false
        holding = false
        holdTimer?.invalidate()
        holdTimer = Timer.scheduledTimer(withTimeInterval: 0.35, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, !self.dragging else { return }
                self.holding = true
                self.onHoldBegan()
            }
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard !holding, let window, let last = lastScreenPoint else { return }
        let now = NSEvent.mouseLocation
        let dx = now.x - last.x
        let dy = now.y - last.y
        if !dragging {
            guard hypot(dx, dy) >= 4 else { return }
            dragging = true
            holdTimer?.invalidate()
        }
        var origin = window.frame.origin
        origin.x += dx
        origin.y += dy
        window.setFrameOrigin(origin)
        lastScreenPoint = now
    }

    override func mouseUp(with event: NSEvent) {
        holdTimer?.invalidate()
        if holding {
            holding = false
            onHoldEnded()
        } else if dragging {
            onDragEnded()
        } else {
            onClick()
        }
        dragging = false
        lastScreenPoint = nil
    }

    override func rightMouseDown(with event: NSEvent) {
        onRightClick(event)
    }
}
