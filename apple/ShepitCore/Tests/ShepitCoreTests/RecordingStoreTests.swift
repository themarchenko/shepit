import Foundation
import XCTest
@testable import ShepitCore

final class RecordingStoreTests: XCTestCase {
    func testWavRoundTripKeepsLengthAndValues() throws {
        let samples: [Float] = (0..<16_000).map { i in 0.5 * sinf(Float(i) * 2 * .pi * 440 / 16_000) }
        let data = Data(wav16kMono: samples)
        XCTAssertEqual(data.count, 44 + samples.count * 2)
        XCTAssertEqual(String(decoding: data.prefix(4), as: UTF8.self), "RIFF")
        XCTAssertEqual(String(decoding: data[8..<12], as: UTF8.self), "WAVE")

        let url = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-test-\(UUID().uuidString).wav")
        defer { try? FileManager.default.removeItem(at: url) }
        try data.write(to: url)
        let back = try AudioFile.loadSamples16k(url)
        XCTAssertEqual(back.count, samples.count)
        for i in stride(from: 0, to: samples.count, by: 997) {
            XCTAssertEqual(back[i], samples[i], accuracy: 1.0 / 32_000)
        }
    }

    func testFileNamesSortChronologically() {
        let earlier = RecordingStore.fileName(for: Date(timeIntervalSince1970: 1_700_000_000))
        let later = RecordingStore.fileName(for: Date(timeIntervalSince1970: 1_700_000_001))
        XCTAssertLessThan(earlier, later)
        XCTAssertTrue(earlier.hasSuffix(".wav"))
    }
}
