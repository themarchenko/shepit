import Foundation

/// Keeps the audio of recent dictations as 16 kHz mono 16-bit WAV files, so recognizers can be
/// compared on the user's own voice and corrections can later be paired with what was said.
public enum RecordingStore {
    /// How many recordings to keep; the oldest are deleted after each save.
    public static let keepCount = 200

    /// File names sort chronologically, which `prune` relies on.
    public static func fileName(for date: Date = Date()) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss-SSS"
        return formatter.string(from: date) + ".wav"
    }

    public static func url(for fileName: String) -> URL {
        AppPaths.recordingsDirectory.appendingPathComponent(fileName)
    }

    @discardableResult
    public static func save(_ samples: [Float], as fileName: String) throws -> URL {
        let target = url(for: fileName)
        try Data(wav16kMono: samples).write(to: target, options: .atomic)
        return target
    }

    public static func prune(keep: Int = keepCount) {
        let fm = FileManager.default
        guard let names = try? fm.contentsOfDirectory(atPath: AppPaths.recordingsDirectory.path) else { return }
        let wavs = names.filter { $0.hasSuffix(".wav") }.sorted()
        guard wavs.count > keep else { return }
        for name in wavs.prefix(wavs.count - keep) {
            try? fm.removeItem(at: url(for: name))
        }
    }

    /// Removes every regular file in the folder: WAVs, half-written `.wav.tmp` and whatever a crashed
    /// atomic save left. Subdirectories stay. A missing folder is zero files; any other listing error
    /// throws. A failed removal does not stop the rest; the first such error is thrown at the end.
    /// Returns how many files were removed.
    @discardableResult
    public static func clear(in dir: URL) throws -> Int {
        let fm = FileManager.default
        let entries: [URL]
        do {
            // Sorted so a partial failure leaves the same files every time.
            entries = try fm.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.isRegularFileKey])
                .sorted { $0.lastPathComponent < $1.lastPathComponent }
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile || error.code == .fileNoSuchFile {
            return 0
        }
        var removed = 0
        var firstError: Error?
        for entry in entries {
            guard (try? entry.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else { continue }
            do {
                try fm.removeItem(at: entry)
                removed += 1
            } catch {
                firstError = firstError ?? error
            }
        }
        if let firstError { throw firstError }
        return removed
    }

    public static func allRecordings() -> [URL] {
        let fm = FileManager.default
        let names = (try? fm.contentsOfDirectory(atPath: AppPaths.recordingsDirectory.path)) ?? []
        return names.filter { $0.hasSuffix(".wav") }.sorted().map(url(for:))
    }
}

extension Data {
    /// A canonical 44-byte-header RIFF/WAVE file with 16-bit PCM mono at 16 kHz.
    init(wav16kMono samples: [Float]) {
        let sampleRate: UInt32 = 16_000
        let dataSize = UInt32(samples.count * 2)
        var data = Data(capacity: 44 + Int(dataSize))
        func append<T: FixedWidthInteger>(_ value: T) {
            var little = value.littleEndian
            Swift.withUnsafeBytes(of: &little) { data.append(contentsOf: $0) }
        }
        data.append(contentsOf: Array("RIFF".utf8))
        append(UInt32(36 + dataSize))
        data.append(contentsOf: Array("WAVE".utf8))
        data.append(contentsOf: Array("fmt ".utf8))
        append(UInt32(16))          // fmt chunk size
        append(UInt16(1))           // PCM
        append(UInt16(1))           // mono
        append(sampleRate)
        append(sampleRate * 2)      // byte rate
        append(UInt16(2))           // block align
        append(UInt16(16))          // bits per sample
        data.append(contentsOf: Array("data".utf8))
        append(dataSize)
        var pcm = [Int16](repeating: 0, count: samples.count)
        for (i, sample) in samples.enumerated() {
            pcm[i] = Int16((Swift.max(-1, Swift.min(1, sample)) * 32767).rounded())
        }
        pcm.withUnsafeBufferPointer { data.append(Data(buffer: $0)) }
        self = data
    }
}
