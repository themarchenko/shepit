import XCTest
@testable import ShepitCore

/// Keeps `shared/prompts/system-prompt.md` equal to what the app sends. On a mismatch the actual text goes to
/// the temporary directory; review it and copy it over the golden by hand. Nothing here writes into `shared/`.
final class PromptGoldenTests: XCTestCase {
    func testSystemPromptMatchesGolden() throws {
        let url = SharedFixtures.root
            .appendingPathComponent("shared").appendingPathComponent("prompts")
            .appendingPathComponent("system-prompt.md")
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw FixtureError(description: "missing golden \(url.path)")
        }
        let golden = try Data(contentsOf: url)
        let actual = Data((PromptBuilder.systemPrompt(speakerNotes: SpeakerNotes.defaultText) + "\n").utf8)
        if actual != golden {
            let out = FileManager.default.temporaryDirectory.appendingPathComponent("system-prompt.actual.md")
            try actual.write(to: out)
            XCTFail("system prompt differs from \(url.path); actual text written to \(out.path)")
        }
    }
}
