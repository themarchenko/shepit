import XCTest

/// Reads the three String Catalogs straight from the source tree. A missing file fails the test.
final class LocalizationCatalogTests: XCTestCase {
    private static let apple = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent()

    private static let catalogPaths = [
        "ShepitCore/Sources/ShepitCore/Resources/Localizable.xcstrings",
        "macOS/Shepit/Localizable.xcstrings",
        "macOS/Shepit/InfoPlist.xcstrings",
    ]

    private struct Entry {
        let catalog: String
        let key: String
        let raw: [String: Any]
        /// Every uk string with a label ("value" or the plural category).
        let uk: [(String, String)]
        let ukComplete: Bool
        let hasUk: Bool
    }

    private func load(_ relative: String) throws -> [String: Any] {
        let url = Self.apple.appendingPathComponent(relative)
        let data = try Data(contentsOf: url)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any], relative)
    }

    private func unit(_ any: Any?) -> (state: String, value: String)? {
        guard let u = (any as? [String: Any])?["stringUnit"] as? [String: Any] else { return nil }
        return (u["state"] as? String ?? "", u["value"] as? String ?? "")
    }

    private func entries() throws -> [Entry] {
        var out: [Entry] = []
        for path in Self.catalogPaths {
            let catalog = try load(path)
            let strings = catalog["strings"] as? [String: Any] ?? [:]
            for (key, value) in strings {
                let raw = value as? [String: Any] ?? [:]
                if raw["shouldTranslate"] as? Bool == false { continue }
                let uk = (raw["localizations"] as? [String: Any])?["uk"] as? [String: Any]
                var pairs: [(String, String)] = []
                var complete = false
                if let u = unit(uk) {
                    pairs = [("value", u.value)]
                    complete = u.state == "translated" && !u.value.isEmpty
                } else if let plural = (uk?["variations"] as? [String: Any])?["plural"] as? [String: Any] {
                    for category in ["one", "few", "many", "other"] {
                        if let u = unit(plural[category]) {
                            pairs.append((category, u.value))
                        }
                    }
                    complete = ["one", "few", "many", "other"].allSatisfy {
                        if let u = unit(plural[$0]) { return u.state == "translated" && !u.value.isEmpty }
                        return false
                    }
                }
                out.append(Entry(catalog: path, key: key, raw: raw, uk: pairs, ukComplete: complete, hasUk: uk != nil))
            }
        }
        return out
    }

    /// Types of the format specifiers in `s`, positional markers and length modifiers removed.
    private func specifiers(_ s: String) -> [String] {
        let regex = try! NSRegularExpression(pattern: "%(?:\\d+\\$)?(?:hh|h|ll|l|z|q)?([@dDiuUxXfFeEgGsScC])")
        let ns = s as NSString
        return regex.matches(in: s, range: NSRange(location: 0, length: ns.length)).map { m in
            let c = ns.substring(with: m.range(at: 1))
            return ["i", "D"].contains(c) ? "d" : c
        }
    }

    // MARK: Catalog shape

    func testCatalogsExistAndAreEnglishSourced() throws {
        for path in Self.catalogPaths {
            let catalog = try load(path)
            XCTAssertEqual(catalog["sourceLanguage"] as? String, "en", path)
            XCTAssertNotNil(catalog["strings"] as? [String: Any], path)
        }
    }

    func testInfoPlistCatalogTranslatesMicrophoneDescription() throws {
        let strings = try load("macOS/Shepit/InfoPlist.xcstrings")["strings"] as? [String: Any]
        let entry = strings?["NSMicrophoneUsageDescription"] as? [String: Any]
        let locs = entry?["localizations"] as? [String: Any]
        XCTAssertFalse(unit(locs?["en"])?.value.isEmpty ?? true, "en value missing")
        XCTAssertFalse(unit(locs?["uk"])?.value.isEmpty ?? true, "uk value missing")
    }

    func testCoreCatalogIsNotEmpty() throws {
        let strings = try load(Self.catalogPaths[0])["strings"] as? [String: Any]
        XCTAssertGreaterThan(strings?.count ?? 0, 40)
    }

    func testEveryEntryHasCompleteUkrainian() throws {
        for e in try entries() {
            XCTAssertTrue(e.hasUk, "\(e.catalog): no uk for \(e.key)")
            XCTAssertTrue(e.ukComplete, "\(e.catalog): uk incomplete for \(e.key)")
        }
    }

    func testNoStaleEntries() throws {
        for e in try entries() {
            XCTAssertNotEqual(e.raw["extractionState"] as? String, "stale", "\(e.catalog): stale \(e.key)")
        }
    }

    // MARK: Text rules (one test per rule)

    func testUkrainianSpecifiersMatchTheKey() throws {
        for e in try entries() {
            var remaining = specifiers(e.key)
            for (label, value) in e.uk {
                remaining = specifiers(e.key)
                for spec in specifiers(value) {
                    if let i = remaining.firstIndex(of: spec) { remaining.remove(at: i) }
                    else { XCTFail("\(e.catalog): \(e.key) [\(label)] has specifier %\(spec) not in the key: \(value)") }
                }
            }
        }
    }

    func testUkrainianHasNoBannedStems() throws {
        let banned = ["налаштуван", "диктовк", "полірув"]
        for e in try entries() {
            for (label, value) in e.uk {
                for stem in banned where value.lowercased().contains(stem) {
                    XCTFail("\(e.catalog): \(e.key) [\(label)] contains \(stem)")
                }
            }
        }
    }

    func testUkrainianHasNoEmDash() throws {
        for e in try entries() {
            for (label, value) in e.uk where value.contains("\u{2014}") {
                XCTFail("\(e.catalog): \(e.key) [\(label)] has an em dash")
            }
        }
    }

    func testUkrainianHasNoAsciiApostrophe() throws {
        for e in try entries() {
            for (label, value) in e.uk where value.contains("'") {
                XCTFail("\(e.catalog): \(e.key) [\(label)] has an ASCII apostrophe, use U+2019")
            }
        }
    }

    // MARK: Source stays English

    func testOnlyAllowlistedSourcesContainCyrillic() throws {
        let allowed: Set<String> = [
            "PromptBuilder", "SpeechPresence", "ScriptCleanup", "DictationSession", "DictionaryCorrector",
            "CalibrationMessages", "CalibrationScript", "Smoke", "WidgetDemo",
        ]
        var scanned = 0
        for root in ["macOS/Shepit", "ShepitCore/Sources"] {
            let dir = Self.apple.appendingPathComponent(root)
            let walker = try XCTUnwrap(FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil), root)
            for case let url as URL in walker where url.pathExtension == "swift" {
                scanned += 1
                let text = try String(contentsOf: url, encoding: .utf8)
                let cyrillic = text.unicodeScalars.contains { (0x0400...0x04FF).contains($0.value) }
                let name = url.deletingPathExtension().lastPathComponent
                if cyrillic { XCTAssertTrue(allowed.contains(name), "\(url.path) contains Cyrillic") }
            }
        }
        XCTAssertGreaterThan(scanned, 50)
    }
}
