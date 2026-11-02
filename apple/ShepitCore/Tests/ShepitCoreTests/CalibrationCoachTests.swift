import XCTest
@testable import ShepitCore

final class FakeCalibrationAgent: CalibrationAgent, @unchecked Sendable {
    private let lock = NSLock()
    private var log: [(message: String, round: CalibrationRound)] = []
    var result: Result<String, Error>

    init(_ result: Result<String, Error>) { self.result = result }

    var calls: [(message: String, round: CalibrationRound)] { lock.withLock { log } }

    func reply(to message: String, round: CalibrationRound) async throws -> String {
        lock.withLock { log.append((message, round)) }
        return try result.get()
    }
}

final class CalibrationCoachTests: XCTestCase {
    private let policy = LanguagePolicy.ukrainianEnglish
    private let pairs = [
        CalibrationPair(reference: "рефакторинг", heard: "рефакторінг", raw: "рефакторінг"),
        CalibrationPair(reference: "branch", heard: "бранч", raw: "бранч"),
        CalibrationPair(reference: "Лілія читала лист.", heard: "Лія читала лист.", raw: "Лія читала лист."),
    ]
    private struct Boom: Error {}

    func testTargetsUseAgentReply() async {
        let agent = FakeCalibrationAgent(.success(#"{"sentences": ["Лілія повільно читала лист біля вікна."]}"#))
        let (prompts, used) = await CalibrationCoach(agent: agent, policy: policy).targets(pairs: pairs)
        XCTAssertTrue(used)
        XCTAssertEqual(prompts.map(\.text), ["Лілія повільно читала лист біля вікна."])
        XCTAssertEqual(agent.calls.map(\.round), [.targets])
        XCTAssertEqual(agent.calls.first?.message,
                       CalibrationMessages.request(round: .targets, policy: policy, pairs: pairs))
    }

    func testTargetsFallBackOnThrowNilAgentOrBadReply() async {
        let fallback = CalibrationScript.fallback(for: policy)
        for agent in [FakeCalibrationAgent(.failure(Boom())), FakeCalibrationAgent(.success("not json"))] {
            let (prompts, used) = await CalibrationCoach(agent: agent, policy: policy).targets(pairs: pairs)
            XCTAssertFalse(used)
            XCTAssertEqual(prompts, fallback)
        }
        let (prompts, used) = await CalibrationCoach(agent: nil, policy: policy).targets(pairs: pairs)
        XCTAssertFalse(used)
        XCTAssertEqual(prompts, fallback)
    }

    func testTargetsSkipAgentWithNoPairs() async {
        let agent = FakeCalibrationAgent(.success(#"{"sentences": ["Лілія повільно читала лист біля вікна."]}"#))
        let (prompts, used) = await CalibrationCoach(agent: agent, policy: policy).targets(pairs: [])
        XCTAssertFalse(used)
        XCTAssertEqual(prompts, CalibrationScript.fallback(for: policy))
        XCTAssertTrue(agent.calls.isEmpty)
    }

    func testProfileSendsAllPairsRoundProfile() async throws {
        let agent = FakeCalibrationAgent(.success(#"{"notes": "Speaks Ukrainian.", "terms": ["Rebase"]}"#))
        let profile = try await CalibrationCoach(agent: agent, policy: policy)
            .profile(pairs: pairs, existingDictionary: [])
        XCTAssertEqual(profile.notes, "Speaks Ukrainian.")
        XCTAssertEqual(profile.terms, ["Rebase"])
        XCTAssertEqual(agent.calls.map(\.round), [.profile])
        XCTAssertEqual(agent.calls.first?.message,
                       CalibrationMessages.request(round: .profile, policy: policy, pairs: pairs))
    }

    func testProfileThrowsTooFewTakes() async {
        let agent = FakeCalibrationAgent(.success(#"{"notes": "x", "terms": []}"#))
        do {
            _ = try await CalibrationCoach(agent: agent, policy: policy)
                .profile(pairs: Array(pairs.prefix(2)), existingDictionary: [])
            XCTFail("expected tooFewTakes")
        } catch {
            XCTAssertEqual(error as? CalibrationError, .tooFewTakes)
        }
        XCTAssertTrue(agent.calls.isEmpty)
    }

    func testProfileThrowsOnBadReply() async {
        let agent = FakeCalibrationAgent(.success("no json"))
        do {
            _ = try await CalibrationCoach(agent: agent, policy: policy).profile(pairs: pairs, existingDictionary: [])
            XCTFail("expected badReply")
        } catch {
            XCTAssertEqual(error as? CalibrationError, .badReply)
        }
    }
}
