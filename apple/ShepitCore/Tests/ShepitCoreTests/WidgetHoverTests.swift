import CoreGraphics
import XCTest
@testable import ShepitCore

final class WidgetHoverTests: XCTestCase {
    private let frame = CGRect(x: 100, y: 50, width: 200, height: 80)

    func testPointerOutsideWhileHoveringTurnsHoverOff() {
        XCTAssertEqual(WidgetHover.change(hovering: true, panelFrame: frame, pointer: CGPoint(x: 500, y: 500)), false)
    }

    func testPointerInsideWhileNotHoveringTurnsHoverOn() {
        XCTAssertEqual(WidgetHover.change(hovering: false, panelFrame: frame, pointer: CGPoint(x: 150, y: 90)), true)
    }

    func testNoChangeWhenStateMatchesPointer() {
        XCTAssertNil(WidgetHover.change(hovering: true, panelFrame: frame, pointer: CGPoint(x: 150, y: 90)))
        XCTAssertNil(WidgetHover.change(hovering: false, panelFrame: frame, pointer: CGPoint(x: 500, y: 500)))
    }

    func testShrinkUnderStillPointerTurnsHoverOff() {
        // Same bottom-middle anchor, the pointer stays where it was.
        let large = CGRect(x: 100, y: 50, width: 200, height: 80)
        let small = CGRect(x: 160, y: 50, width: 80, height: 30)
        let pointer = CGPoint(x: 110, y: 120)
        XCTAssertNil(WidgetHover.change(hovering: true, panelFrame: large, pointer: pointer))
        XCTAssertEqual(WidgetHover.change(hovering: true, panelFrame: small, pointer: pointer), false)
    }

    func testShadowMarginCountsAsInside() {
        let pointer = CGPoint(x: frame.minX + 3, y: frame.minY + 3)
        XCTAssertNil(WidgetHover.change(hovering: true, panelFrame: frame, pointer: pointer))
    }

    func testEdgesAreHalfOpen() {
        XCTAssertEqual(WidgetHover.change(hovering: false, panelFrame: frame, pointer: CGPoint(x: frame.minX, y: frame.minY)), true)
        XCTAssertNil(WidgetHover.change(hovering: false, panelFrame: frame, pointer: CGPoint(x: frame.maxX, y: frame.midY)))
        XCTAssertNil(WidgetHover.change(hovering: false, panelFrame: frame, pointer: CGPoint(x: frame.midX, y: frame.maxY)))
    }

    func testEmptyFrameNeverHovers() {
        XCTAssertEqual(WidgetHover.change(hovering: true, panelFrame: .zero, pointer: .zero), false)
    }
}
