import XCTest
@testable import ShepitCore

final class HotkeyGestureTests: XCTestCase {
    private func assertPressEnded(_ output: HotkeyGesture.Output, duration: TimeInterval, file: StaticString = #filePath, line: UInt = #line) {
        guard case .pressEnded(let d) = output else {
            XCTFail("expected pressEnded, got \(output)", file: file, line: line)
            return
        }
        XCTAssertEqual(d, duration, accuracy: 0.001, file: file, line: line)
    }

    func testLongPressIsPushToTalk() {
        var g = HotkeyGesture()
        let t = Date()
        XCTAssertEqual(g.keyDown(at: t), .pressBegan)
        XCTAssertEqual(g.keyDown(at: t.addingTimeInterval(0.1)), .none, "key repeat is ignored")
        assertPressEnded(g.keyUp(at: t.addingTimeInterval(1.2)), duration: 1.2)
    }

    func testShortTapThenDoubleTap() {
        var g = HotkeyGesture()
        let t = Date()
        _ = g.keyDown(at: t)
        XCTAssertEqual(g.keyUp(at: t.addingTimeInterval(0.1)), .shortTap)
        _ = g.keyDown(at: t.addingTimeInterval(0.3))
        XCTAssertEqual(g.keyUp(at: t.addingTimeInterval(0.4)), .doubleTap)
        // The double tap resets the window: a third tap is a plain short tap again.
        _ = g.keyDown(at: t.addingTimeInterval(0.6))
        XCTAssertEqual(g.keyUp(at: t.addingTimeInterval(0.7)), .shortTap)
    }

    func testTapsTooFarApartAreNotADoubleTap() {
        var g = HotkeyGesture()
        let t = Date()
        _ = g.keyDown(at: t)
        _ = g.keyUp(at: t.addingTimeInterval(0.1))
        _ = g.keyDown(at: t.addingTimeInterval(1.0))
        XCTAssertEqual(g.keyUp(at: t.addingTimeInterval(1.1)), .shortTap)
    }

    func testLongPressClearsPendingTap() {
        var g = HotkeyGesture()
        let t = Date()
        _ = g.keyDown(at: t)
        _ = g.keyUp(at: t.addingTimeInterval(0.1))
        _ = g.keyDown(at: t.addingTimeInterval(0.2))
        assertPressEnded(g.keyUp(at: t.addingTimeInterval(1.0)), duration: 0.8)
        _ = g.keyDown(at: t.addingTimeInterval(1.1))
        XCTAssertEqual(g.keyUp(at: t.addingTimeInterval(1.2)), .shortTap)
    }
}
