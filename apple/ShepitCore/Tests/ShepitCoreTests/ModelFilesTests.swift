import XCTest
@testable import ShepitCore

final class ModelFilesTests: XCTestCase {
    private let model = "large-v3-v20240930_turbo"
    private var base: URL!

    override func setUpWithError() throws {
        base = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-modelfiles-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: base)
    }

    private func folder() -> URL {
        base.appendingPathComponent("models/argmaxinc/whisperkit-coreml/openai_whisper-\(model)", isDirectory: true)
    }

    private func make(_ names: [String]) throws {
        for n in names {
            try FileManager.default.createDirectory(at: folder().appendingPathComponent("\(n).mlmodelc"), withIntermediateDirectories: true)
        }
    }

    func testWhisperFolderPresentNeedsAllThreeModels() throws {
        XCTAssertFalse(WhisperKitEngine.modelFolderPresent(base: base, model: model))
        try make(["AudioEncoder"])
        XCTAssertFalse(WhisperKitEngine.modelFolderPresent(base: base, model: model))
        try make(["TextDecoder"])
        XCTAssertFalse(WhisperKitEngine.modelFolderPresent(base: base, model: model))
        try make(["MelSpectrogram"])
        XCTAssertTrue(WhisperKitEngine.modelFolderPresent(base: base, model: model))
    }
}
