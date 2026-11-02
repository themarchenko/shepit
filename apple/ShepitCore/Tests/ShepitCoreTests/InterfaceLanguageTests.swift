import XCTest
@testable import ShepitCore

/// The app-language choice is stored as AppleLanguages in the app's own defaults domain, so macOS
/// itself picks the localization on the next launch. `AppSettings` reads that domain, never the global one.
@MainActor
final class InterfaceLanguageTests: XCTestCase {
    private var suite = ""
    private var defaults: UserDefaults!

    override func setUpWithError() throws {
        try super.setUpWithError()
        try CoreLocalizationFixture.install()
        suite = "shepit.test.interface-language.\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suite)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suite)
        CoreLocalizationFixture.reset()
        super.tearDown()
    }

    private func settings() -> AppSettings { AppSettings(defaults: defaults, domain: suite) }

    private func storedLanguages() -> [String]? {
        defaults.persistentDomain(forName: suite)?["AppleLanguages"] as? [String]
    }

    // MARK: Reading

    func testNoKeyMeansSystem() {
        XCTAssertEqual(settings().interfaceLanguage, .system)
    }

    func testUkrainianFirstInStoredListMeansUkrainian() {
        defaults.set(["uk-UA", "en"], forKey: "AppleLanguages")
        XCTAssertEqual(settings().interfaceLanguage, .ukrainian)
    }

    func testEnglishStoredMeansEnglish() {
        defaults.set(["en"], forKey: "AppleLanguages")
        XCTAssertEqual(settings().interfaceLanguage, .english)
    }

    func testUnsupportedStoredLanguageMeansSystemAndIsNotRewritten() {
        defaults.set(["de"], forKey: "AppleLanguages")
        XCTAssertEqual(settings().interfaceLanguage, .system)
        XCTAssertEqual(storedLanguages(), ["de"])
    }

    // MARK: Writing

    func testChoosingUkrainianWritesExactlyUk() {
        let s = settings()
        s.interfaceLanguage = .ukrainian
        XCTAssertEqual(storedLanguages(), ["uk"])
    }

    func testChoosingEnglishWritesExactlyEn() {
        let s = settings()
        s.interfaceLanguage = .english
        XCTAssertEqual(storedLanguages(), ["en"])
    }

    func testChoosingSystemRemovesTheOverride() {
        let s = settings()
        s.interfaceLanguage = .ukrainian
        s.interfaceLanguage = .system
        XCTAssertNil(defaults.persistentDomain(forName: suite)?["AppleLanguages"])
    }

    // MARK: Pure rules

    func testAvailableLanguages() {
        XCTAssertEqual(InterfaceLanguage.available, ["en", "uk"])
    }

    func testRawValues() {
        XCTAssertEqual(InterfaceLanguage.english.rawValue, "en")
        XCTAssertEqual(InterfaceLanguage.ukrainian.rawValue, "uk")
    }

    func testTargetPicksFirstAvailableSystemLanguage() {
        XCTAssertEqual(InterfaceLanguage.target(system: ["uk-UA", "en"]), "uk")
        XCTAssertEqual(InterfaceLanguage.target(system: ["de-DE", "uk-UA"]), "uk")
        XCTAssertEqual(InterfaceLanguage.target(system: ["en-GB"]), "en")
    }

    func testTargetFallsBackToEnglish() {
        XCTAssertEqual(InterfaceLanguage.target(system: ["de-DE", "fr"]), "en")
        XCTAssertEqual(InterfaceLanguage.target(system: []), "en")
    }

    func testRestartNeededWhenChoiceDiffersFromRunningLanguage() {
        XCTAssertTrue(InterfaceLanguage.ukrainian.needsRestart(running: "en", system: ["en-US"]))
        XCTAssertFalse(InterfaceLanguage.english.needsRestart(running: "en", system: ["uk-UA"]))
    }

    func testSystemChoiceFollowsFirstSupportedSystemLanguage() {
        XCTAssertFalse(InterfaceLanguage.system.needsRestart(running: "uk", system: ["uk-UA"]))
        XCTAssertTrue(InterfaceLanguage.system.needsRestart(running: "uk", system: ["de-DE"]))
    }

    // MARK: Names

    func testEndonymsAreTheSameInBothLanguages() {
        let ukrainian = "\u{0423}\u{043A}\u{0440}\u{0430}\u{0457}\u{043D}\u{0441}\u{044C}\u{043A}\u{0430}"
        for code in ["en", "uk"] {
            CoreLocalization.override = code
            XCTAssertEqual(InterfaceLanguage.english.displayName, "English", code)
            XCTAssertEqual(InterfaceLanguage.ukrainian.displayName, ukrainian, code)
        }
    }

    func testSystemNameIsLocalized() {
        CoreLocalization.override = "en"
        XCTAssertEqual(InterfaceLanguage.system.displayName, "System")
        CoreLocalization.override = "uk"
        XCTAssertTrue(InterfaceLanguage.system.displayName.unicodeScalars.contains { (0x0400...0x04FF).contains($0.value) })
    }
}
