import XCTest
@testable import ShepitCore

final class CalibrationMessagesTests: XCTestCase {
    private func json(_ text: String) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]) ?? [:]
    }

    private func reply(sentences: [String]) -> String {
        let data = try! JSONSerialization.data(withJSONObject: ["sentences": sentences])
        return String(decoding: data, as: UTF8.self)
    }

    private func reply(notes: String, terms: [String]) -> String {
        let data = try! JSONSerialization.data(withJSONObject: ["notes": notes, "terms": terms])
        return String(decoding: data, as: UTF8.self)
    }

    func testRequestKeysOnly() throws {
        let pairs = [
            CalibrationPair(reference: "рефакторинг", heard: "рефакторінг", raw: "рефакторінг"),
            CalibrationPair(reference: "Ми вирішили", heard: "Ми вирішили", raw: "Мы вирішили"),
        ]
        let text = CalibrationMessages.request(round: .targets, policy: .ukrainianEnglish, pairs: pairs)
        let object = json(text)
        XCTAssertEqual(Set(object.keys), ["round", "languages", "takes"])
        XCTAssertEqual(object["round"] as? String, "targets")
        XCTAssertEqual(object["languages"] as? [String], ["uk", "en"])
        let takes = try XCTUnwrap(object["takes"] as? [[String: Any]])
        XCTAssertEqual(takes.count, 2)
        for take in takes {
            XCTAssertTrue(Set(take.keys).isSubset(of: ["reference", "heard", "raw"]), "\(take.keys)")
        }
        XCTAssertNil(takes[0]["raw"], "raw is left out when it equals heard")
        XCTAssertEqual(takes[1]["raw"] as? String, "Мы вирішили")
        XCTAssertEqual(takes[1]["heard"] as? String, "Ми вирішили")
        XCTAssertEqual(takes[0]["reference"] as? String, "рефакторинг")

        let profile = json(CalibrationMessages.request(round: .profile, policy: .english, pairs: pairs))
        XCTAssertEqual(profile["round"] as? String, "profile")
        XCTAssertEqual(profile["languages"] as? [String], ["en"])
    }

    func testSystemPromptNamesBothRoundsAndForbidsRussianPolish() {
        let prompt = CalibrationMessages.systemPrompt
        XCTAssertTrue(prompt.contains("targets"))
        XCTAssertTrue(prompt.contains("profile"))
        XCTAssertTrue(prompt.contains("Russian"))
        XCTAssertTrue(prompt.contains("Polish"))
        XCTAssertFalse(prompt.contains("\u{2014}"), "no em dashes")
    }

    func testTargetsParse() throws {
        let fenced = "```json\n" + reply(sentences: ["Рома розробив реліз швидко і чітко сьогодні.", "Please rerun the failing tests on the release branch."]) + "\n```"
        let parsed = try XCTUnwrap(CalibrationMessages.parseTargets(fenced, policy: .ukrainianEnglish))
        XCTAssertEqual(parsed.map(\.text), ["Рома розробив реліз швидко і чітко сьогодні.", "Please rerun the failing tests on the release branch."])
        XCTAssertEqual(Set(parsed.map(\.id)).count, parsed.count)

        let wrapped = "Here you go: " + reply(sentences: ["Лілія повільно читала лист біля вікна."]) + " Done."
        XCTAssertEqual(CalibrationMessages.parseTargets(wrapped, policy: .ukrainian)?.count, 1)

        let many = reply(sentences: (1...5).map { "Sentence number \($0) is about the new library today." })
        XCTAssertEqual(CalibrationMessages.parseTargets(many, policy: .english)?.count, 3)
    }

    func testTargetsDropInvalid() {
        let good = "Лілія повільно читала лист біля вікна."
        let bad = [
            "Это предложение содержит русские буквы.",
            "Zażółć gęślą jaźń in a long sentence today.",
            "",
            String(repeating: "word ", count: 50),
        ]
        let parsed = CalibrationMessages.parseTargets(reply(sentences: bad + [good]), policy: .ukrainian)
        XCTAssertEqual(parsed?.map(\.text), [good])

        XCTAssertNil(CalibrationMessages.parseTargets(reply(sentences: bad), policy: .ukrainian), "all dropped is nil")
        XCTAssertNil(CalibrationMessages.parseTargets(reply(sentences: [good]), policy: .english),
                     "Cyrillic is dropped under English only")
    }

    func testTargetsMalformed() {
        XCTAssertNil(CalibrationMessages.parseTargets("no json at all", policy: .ukrainianEnglish))
        XCTAssertNil(CalibrationMessages.parseTargets(#"{"sentences": "one sentence"}"#, policy: .ukrainianEnglish))
        XCTAssertNil(CalibrationMessages.parseTargets(#"{"sentences": [1, 2, null]}"#, policy: .ukrainianEnglish))
        XCTAssertNil(CalibrationMessages.parseTargets(#"{"other": []}"#, policy: .ukrainianEnglish))
    }

    func testProfileParse() throws {
        let profile = try XCTUnwrap(CalibrationMessages.parseProfile(
            reply(notes: "  \nSpeaks Ukrainian and English.\nSwallows л.\n \n", terms: ["Kubernetes"]),
            existingDictionary: []))
        XCTAssertEqual(profile.notes, "Speaks Ukrainian and English.\nSwallows л.")
        XCTAssertEqual(profile.terms, ["Kubernetes"])
    }

    func testProfileNotesTooLongIsNil() {
        XCTAssertNil(CalibrationMessages.parseProfile(reply(notes: String(repeating: "a", count: 2001), terms: []), existingDictionary: []))
        XCTAssertNotNil(CalibrationMessages.parseProfile(reply(notes: String(repeating: "a", count: 2000), terms: []), existingDictionary: []))
    }

    func testProfileWithoutNotesIsNil() {
        XCTAssertNil(CalibrationMessages.parseProfile(#"{"terms": ["x"]}"#, existingDictionary: []))
        XCTAssertNil(CalibrationMessages.parseProfile(reply(notes: "  \n ", terms: ["Shepit"]), existingDictionary: []))
        XCTAssertNil(CalibrationMessages.parseProfile("nothing", existingDictionary: []))
    }

    func testTermsFiltered() throws {
        let terms = [
            "Kubernetes", "kubernetes",      // duplicate by case
            "Shepit",                        // already in the dictionary
            "a",                             // too short
            String(repeating: "x", count: 41),
            "12345",                         // no letter
            "two\nlines",                    // control character
            "one two three four",            // more than 3 words
            "Это",                           // Russian letters
            "Zażółć",                        // Polish letters
            "Лілія", "Rebase",
        ]
        let profile = try XCTUnwrap(CalibrationMessages.parseProfile(reply(notes: "n", terms: terms),
                                                                     existingDictionary: ["shepit"]))
        XCTAssertEqual(profile.terms, ["Kubernetes", "Лілія", "Rebase"])

        let many = (1...20).map { "Term\($0)x" }
        let capped = try XCTUnwrap(CalibrationMessages.parseProfile(reply(notes: "n", terms: many), existingDictionary: []))
        XCTAssertEqual(capped.terms, Array(many.prefix(12)))
    }

    func testMergeDictionary() {
        XCTAssertEqual(CalibrationMessages.mergeDictionary(existing: ["Shepit", "Zed"], adding: ["zed", "Rebase", "rebase", "Lint"]),
                       ["Shepit", "Zed", "Rebase", "Lint"])

        let full = (1..<DictionaryLearning.maxTerms).map { "Existing\($0)" }
        let merged = CalibrationMessages.mergeDictionary(existing: full, adding: ["AddedOne", "AddedTwo"])
        XCTAssertEqual(merged.count, DictionaryLearning.maxTerms)
        XCTAssertEqual(merged.last, "AddedOne")
        XCTAssertFalse(merged.contains("AddedTwo"))
    }
}
