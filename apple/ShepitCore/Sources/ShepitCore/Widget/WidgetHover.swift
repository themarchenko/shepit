import CoreGraphics

/// Decides whether the widget's hover flag disagrees with where the pointer really is.
/// AppKit sends no exit event when the panel shrinks under a still pointer, so the app
/// compares the two itself. Both inputs are global screen coordinates, y up
/// (`NSWindow.frame`, `NSEvent.mouseLocation`).
public enum WidgetHover {
    /// The new hover state when it differs from `hovering`, nil when nothing needs to change.
    public static func change(hovering: Bool, panelFrame: CGRect, pointer: CGPoint) -> Bool? {
        let inside = panelFrame.contains(pointer)
        return inside != hovering ? inside : nil
    }
}
