import Foundation

/// Turns raw key down/up timestamps of the dictation key into gestures.
/// A press longer than `shortTapMax` is push-to-talk. Two short taps within `doubleTapWindow` are a double tap.
public struct HotkeyGesture: Sendable {
    public enum Output: Equatable, Sendable {
        case none
        case pressBegan
        case pressEnded(duration: TimeInterval)
        case shortTap
        case doubleTap
    }

    public var shortTapMax: TimeInterval = 0.25
    public var doubleTapWindow: TimeInterval = 0.45

    private var pressedAt: Date?
    private var lastShortTapAt: Date?

    public init(shortTapMax: TimeInterval = 0.25, doubleTapWindow: TimeInterval = 0.45) {
        self.shortTapMax = shortTapMax
        self.doubleTapWindow = doubleTapWindow
    }

    public mutating func keyDown(at now: Date = Date()) -> Output {
        guard pressedAt == nil else { return .none }
        pressedAt = now
        return .pressBegan
    }

    public mutating func keyUp(at now: Date = Date()) -> Output {
        guard let start = pressedAt else { return .none }
        pressedAt = nil
        let duration = now.timeIntervalSince(start)
        if duration <= shortTapMax {
            if let last = lastShortTapAt, now.timeIntervalSince(last) <= doubleTapWindow {
                lastShortTapAt = nil
                return .doubleTap
            }
            lastShortTapAt = now
            return .shortTap
        }
        lastShortTapAt = nil
        return .pressEnded(duration: duration)
    }

    public var isPressed: Bool { pressedAt != nil }
}
