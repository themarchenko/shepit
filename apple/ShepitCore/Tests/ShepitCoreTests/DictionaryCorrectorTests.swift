import XCTest
@testable import ShepitCore

final class DictionaryCorrectorTests: XCTestCase {
    func testSoundKeysAgreeAcrossScripts() {
        XCTAssertEqual(DictionaryCorrector.soundKey("клод"), DictionaryCorrector.soundKey("Claude"))
        XCTAssertEqual(DictionaryCorrector.soundKey("реквест"), DictionaryCorrector.soundKey("request"))
        XCTAssertEqual(DictionaryCorrector.soundKey("супабейс"), DictionaryCorrector.soundKey("Supabase"))
        XCTAssertEqual(DictionaryCorrector.spokenParts("UserService.swift"), ["User", "Service", "swift"])
    }
}
