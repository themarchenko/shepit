import XCTest
@testable import ShepitCore

@MainActor
final class LanguageSettingTests: XCTestCase {
    private func withDefaults(_ body: (UserDefaults) -> Void) {
        let name = UUID().uuidString
        let defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        body(defaults)
    }

    func testMissingAutoAndUnknownValuesReadAsUkrainianEnglish() {
        withDefaults { defaults in
            XCTAssertEqual(AppSettings(defaults: defaults).language, .ukrainianEnglish)
            defaults.set("auto", forKey: "languageLock")
            XCTAssertEqual(AppSettings(defaults: defaults).language, .ukrainianEnglish)
            defaults.set("xx", forKey: "languageLock")
            XCTAssertEqual(AppSettings(defaults: defaults).language, .ukrainianEnglish)
        }
    }

    func testStoredUkrainianIsKept() {
        withDefaults { defaults in
            defaults.set("uk", forKey: "languageLock")
            XCTAssertEqual(AppSettings(defaults: defaults).language, .ukrainian)
        }
    }

    func testSettingWritesRawValueUnderLanguageLock() {
        withDefaults { defaults in
            let settings = AppSettings(defaults: defaults)
            settings.language = .english
            XCTAssertEqual(defaults.string(forKey: "languageLock"), "en")
            XCTAssertEqual(AppSettings(defaults: defaults).language, .english)
        }
    }

    func testReadingDoesNotRewriteTheStoredValue() {
        withDefaults { defaults in
            defaults.set("auto", forKey: "languageLock")
            _ = AppSettings(defaults: defaults)
            XCTAssertEqual(defaults.string(forKey: "languageLock"), "auto")
        }
    }
}
