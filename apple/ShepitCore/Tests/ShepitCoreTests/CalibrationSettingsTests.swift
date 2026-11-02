import XCTest
@testable import ShepitCore

@MainActor
final class CalibrationSettingsTests: XCTestCase {
    private func defaults() -> UserDefaults { UserDefaults(suiteName: UUID().uuidString)! }

    func testSpeakerNotesDefaultEmpty() {
        XCTAssertEqual(SpeakerNotes.defaultText, "")
        XCTAssertEqual(AppSettings(defaults: defaults()).speakerNotes, "")
        XCTAssertTrue(SpeakerNotes.legacyDefault.contains("Native Ukrainian speaker"))
        XCTAssertTrue(SpeakerNotes.isUnset(""))
        XCTAssertTrue(SpeakerNotes.isUnset(" \n"))
        XCTAssertTrue(SpeakerNotes.isUnset(SpeakerNotes.legacyDefault + "\n"))
        XCTAssertFalse(SpeakerNotes.isUnset("Swallows the letter л."))
    }

    func testCalibrationOfferedDefaultsFalseAndPersists() {
        let store = defaults()
        let first = AppSettings(defaults: store)
        XCTAssertFalse(first.calibrationOffered)
        first.calibrationOffered = true
        XCTAssertTrue(AppSettings(defaults: store).calibrationOffered)
        XCTAssertTrue(store.bool(forKey: "calibrationOffered"))
    }
}
