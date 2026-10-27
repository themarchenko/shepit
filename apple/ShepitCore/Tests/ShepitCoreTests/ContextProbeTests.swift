import ApplicationServices
import XCTest
@testable import ShepitCore

final class ContextProbeTests: XCTestCase {
    func testTextIsReadFromAnOrdinaryField() {
        XCTAssertTrue(ContextProbe.mayReadText(secureEventInput: false, subrole: nil))
        XCTAssertTrue(ContextProbe.mayReadText(secureEventInput: false, subrole: "AXSearchField"))
    }

    func testNoTextIsReadWhileSecureEventInputIsOn() {
        XCTAssertFalse(ContextProbe.mayReadText(secureEventInput: true, subrole: nil))
    }

    func testNoTextIsReadFromASecureTextField() {
        XCTAssertFalse(ContextProbe.mayReadText(secureEventInput: false, subrole: kAXSecureTextFieldSubrole))
    }
}
