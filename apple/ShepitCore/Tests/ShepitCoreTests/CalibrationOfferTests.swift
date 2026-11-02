import XCTest
@testable import ShepitCore

final class CalibrationOfferTests: XCTestCase {
    func testAutoOpenTable() {
        func open(offered: Bool = false, onboarding: Bool = true, tour: Bool = false, doctor: Bool = false,
                  problem: Bool = false, notes: String = "", agent: Bool = true) -> Bool {
            CalibrationOffer.shouldAutoOpen(offered: offered, onboardingCompleted: onboarding, tourOpen: tour,
                                            doctorOpened: doctor, requiredProblem: problem, speakerNotes: notes,
                                            agentAvailable: agent)
        }
        XCTAssertTrue(open())
        XCTAssertFalse(open(offered: true))
        XCTAssertFalse(open(onboarding: false))
        XCTAssertFalse(open(tour: true))
        XCTAssertFalse(open(doctor: true))
        XCTAssertFalse(open(problem: true))
        XCTAssertFalse(open(notes: "Native Ukrainian speaker."))
        XCTAssertFalse(open(agent: false))
        XCTAssertTrue(open(notes: "  \n"))
        XCTAssertTrue(open(notes: SpeakerNotes.legacyDefault))
    }
}
