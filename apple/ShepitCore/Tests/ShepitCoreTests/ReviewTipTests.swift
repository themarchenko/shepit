import XCTest
@testable import ShepitCore

final class ReviewTipTests: XCTestCase {
    private func show(tipShown: Bool = false, reviewBeforePaste: Bool = false, onboardingCompleted: Bool = true,
                      reviewed: Bool = false, method: InsertMethod = .paste) -> Bool {
        ReviewTip.shouldShow(tipShown: tipShown, reviewBeforePaste: reviewBeforePaste,
                             onboardingCompleted: onboardingCompleted,
                             insert: InsertSummary(reviewed: reviewed, method: method))
    }

    func testShowsAfterFirstPlainInsert() {
        for method in [InsertMethod.paste, .accessibility, .typing] {
            XCTAssertTrue(show(method: method), "\(method)")
        }
    }

    func testNotAfterReviewedInsert() { XCTAssertFalse(show(reviewed: true)) }
    func testNotWhenAlreadyShown() { XCTAssertFalse(show(tipShown: true)) }
    func testNotWhenReviewBeforePasteOn() { XCTAssertFalse(show(reviewBeforePaste: true)) }
    func testNotWhenClipboardOnly() { XCTAssertFalse(show(method: .clipboardOnly)) }
    func testNotBeforeOnboardingCompleted() { XCTAssertFalse(show(onboardingCompleted: false)) }
}

@MainActor
final class ReviewTipSettingTests: XCTestCase {
    func testReviewTipShownDefaultsOffAndPersists() {
        let name = UUID().uuidString
        let defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        let first = AppSettings(defaults: defaults)
        XCTAssertFalse(first.reviewTipShown)
        first.reviewTipShown = true
        XCTAssertTrue(AppSettings(defaults: defaults).reviewTipShown)
    }

    func testExistingUserWithoutKeyGetsFalse() {
        let name = UUID().uuidString
        let defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        defaults.set("0.5.0", forKey: "whatsNewSeen")
        defaults.set(true, forKey: "onboardingCompleted")
        XCTAssertFalse(AppSettings(defaults: defaults).reviewTipShown)
    }
}
