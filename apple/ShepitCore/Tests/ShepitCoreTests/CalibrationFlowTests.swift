import XCTest
@testable import ShepitCore

final class CalibrationFlowTests: XCTestCase {
    private let policy = LanguagePolicy.ukrainianEnglish

    private func take(_ text: String, raw: String? = nil) -> CalibrationTake {
        CalibrationTake(raw: raw ?? text, cleaned: text, language: "uk")
    }

    private func started() -> CalibrationFlow {
        var flow = CalibrationFlow()
        flow.start(policy: policy)
        return flow
    }

    /// Records a take for the current prompt and moves on.
    private func answerAndNext(_ flow: inout CalibrationFlow, heard: String? = nil) {
        flow.record(take(heard ?? flow.currentPrompt?.text ?? ""))
        flow.next()
    }

    private func targeted() -> [CalibrationPrompt] {
        [CalibrationPrompt(id: "t1", text: "Перше речення для перевірки.", language: "uk"),
         CalibrationPrompt(id: "t2", text: "Second sentence for the check.", language: "en")]
    }

    /// Walks every word and phrase, recording each, and stops on the sentence-picking stage.
    private func throughPhrases() -> CalibrationFlow {
        var flow = started()
        for _ in 0..<(CalibrationScript.words(for: policy).count + CalibrationScript.phrases(for: policy).count) {
            answerAndNext(&flow)
        }
        return flow
    }

    func testStartGoesToFirstWord() {
        var flow = CalibrationFlow()
        XCTAssertEqual(flow.stage, .intro)
        flow.start(policy: policy)
        XCTAssertEqual(flow.stage, .reading(.words, 0))
        XCTAssertEqual(flow.currentPrompt, CalibrationScript.words(for: policy)[0])
    }

    func testNextWalksWordsThenPhrasesThenPickingSentences() {
        var flow = started()
        let words = CalibrationScript.words(for: policy)
        let phrases = CalibrationScript.phrases(for: policy)
        for i in 0..<words.count {
            XCTAssertEqual(flow.stage, .reading(.words, i))
            XCTAssertEqual(flow.currentPrompt, words[i])
            flow.next()
        }
        for i in 0..<phrases.count {
            XCTAssertEqual(flow.stage, .reading(.phrases, i))
            XCTAssertEqual(flow.currentPrompt, phrases[i])
            flow.next()
        }
        XCTAssertEqual(flow.stage, .pickingSentences)
        XCTAssertNil(flow.currentPrompt)
    }

    func testSkipLeavesNoTake() {
        var flow = started()
        flow.next()
        XCTAssertNil(flow.currentTake)
        flow.back()
        XCTAssertNil(flow.currentTake)
        XCTAssertTrue(flow.pairs(includeTargeted: true).isEmpty)
    }

    func testAgainRemovesTake() {
        var flow = started()
        flow.record(take("рефакторинг"))
        XCTAssertEqual(flow.currentTake?.cleaned, "рефакторинг")
        flow.again()
        XCTAssertNil(flow.currentTake)
        XCTAssertTrue(flow.pairs(includeTargeted: false).isEmpty)
        XCTAssertEqual(flow.stage, .reading(.words, 0), "again stays on the same prompt")
    }

    func testBackStepsToPreviousPromptAndIntro() {
        var flow = started()
        flow.record(take("a"))
        flow.next()
        XCTAssertEqual(flow.stage, .reading(.words, 1))
        flow.back()
        XCTAssertEqual(flow.stage, .reading(.words, 0))
        XCTAssertEqual(flow.currentTake?.cleaned, "a", "a take survives going back")
        flow.back()
        XCTAssertEqual(flow.stage, .intro)

        // Across the words/phrases border.
        var second = started()
        for _ in 0..<CalibrationScript.words(for: policy).count { second.next() }
        XCTAssertEqual(second.stage, .reading(.phrases, 0))
        second.back()
        XCTAssertEqual(second.stage, .reading(.words, CalibrationScript.words(for: policy).count - 1))
    }

    func testNoBackFromFirstTargeted() {
        var flow = throughPhrases()
        XCTAssertEqual(flow.stage, .pickingSentences)
        flow.setTargeted(targeted())
        XCTAssertEqual(flow.stage, .reading(.targeted, 0))
        flow.back()
        XCTAssertEqual(flow.stage, .reading(.targeted, 0))
    }

    func testTargetedThenWritingNotes() {
        var flow = throughPhrases()
        flow.setTargeted(targeted())
        flow.next()
        XCTAssertEqual(flow.stage, .reading(.targeted, 1))
        XCTAssertEqual(flow.currentPrompt?.id, "t2")
        flow.next()
        XCTAssertEqual(flow.stage, .writingNotes)
        flow.profileReady()
        XCTAssertEqual(flow.stage, .review)
    }

    func testPairsInDisplayOrderRecordedOnly() {
        var flow = started()
        let words = CalibrationScript.words(for: policy)
        flow.record(take("w0 heard", raw: "w0 raw"))
        flow.next()
        flow.next() // word 1 skipped
        flow.record(take("w2 heard"))
        let pairs = flow.pairs(includeTargeted: true)
        XCTAssertEqual(pairs, [
            CalibrationPair(reference: words[0].text, heard: "w0 heard", raw: "w0 raw"),
            CalibrationPair(reference: words[2].text, heard: "w2 heard", raw: "w2 heard"),
        ])
    }

    func testRound1PairsExcludeTargeted() {
        var flow = throughPhrases()
        flow.setTargeted(targeted())
        flow.record(take("targeted heard"))
        let all = CalibrationScript.words(for: policy).count + CalibrationScript.phrases(for: policy).count
        XCTAssertEqual(flow.pairs(includeTargeted: false).count, all)
        XCTAssertEqual(flow.pairs(includeTargeted: true).count, all + 1)
        XCTAssertEqual(flow.pairs(includeTargeted: true).last?.reference, targeted()[0].text)
        XCTAssertFalse(flow.pairs(includeTargeted: false).contains { $0.reference == targeted()[0].text })
    }

    func testFailedRetryReturnsToWritingNotes() {
        var flow = throughPhrases()
        flow.setTargeted(targeted())
        flow.next()
        flow.next()
        XCTAssertEqual(flow.stage, .writingNotes)
        flow.profileFailed()
        XCTAssertEqual(flow.stage, .failed)
        flow.retry()
        XCTAssertEqual(flow.stage, .writingNotes)
    }
}
