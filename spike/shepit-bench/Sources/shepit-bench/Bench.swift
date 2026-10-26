// shepit-bench: compares on-device speech recognizers on a folder of 16 kHz WAV recordings.
// Usage: shepit-bench --recordings DIR --phrases DIR --out DIR [--engines parakeet-v3,parakeet-ultra,whisper-turbo]
//        [--whisper-model large-v3-v20240930_turbo] [--skip-missing-reference]

import AVFoundation
import FluidAudio
import Foundation
import WhisperKit

// MARK: - Options

struct Options {
    var recordings = URL(fileURLWithPath: "spike/recordings")
    var phrases = URL(fileURLWithPath: "spike/phrases")
    var out = URL(fileURLWithPath: "spike/results")
    var engines = ["parakeet-v3", "parakeet-ultra", "whisper-turbo"]
    var whisperModel = "large-v3-v20240930_turbo"

    static func parse(_ args: [String]) -> Options {
        var o = Options()
        var i = 0
        func value() -> String {
            i += 1
            guard i < args.count else { fail("missing value for \(args[i - 1])") }
            return args[i]
        }
        while i < args.count {
            switch args[i] {
            case "--recordings": o.recordings = URL(fileURLWithPath: value())
            case "--phrases": o.phrases = URL(fileURLWithPath: value())
            case "--out": o.out = URL(fileURLWithPath: value())
            case "--engines": o.engines = value().split(separator: ",").map { String($0).trimmingCharacters(in: .whitespaces) }
            case "--whisper-model": o.whisperModel = value()
            case "-h", "--help":
                print("shepit-bench --recordings DIR --phrases DIR --out DIR [--engines a,b,c] [--whisper-model NAME]")
                exit(0)
            default: fail("unknown argument \(args[i])")
            }
            i += 1
        }
        return o
    }
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(("error: " + message + "\n").data(using: .utf8)!)
    exit(2)
}

func log(_ message: String) {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
}

// MARK: - Audio

func loadSamples16k(_ url: URL) throws -> [Float] {
    let file = try AVAudioFile(forReading: url)
    let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
    let source = file.processingFormat
    let frameCount = AVAudioFrameCount(file.length)
    guard let input = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: frameCount) else { throw BenchError.audio("buffer") }
    try file.read(into: input)
    if source.sampleRate == 16_000, source.channelCount == 1, source.commonFormat == .pcmFormatFloat32 {
        return Array(UnsafeBufferPointer(start: input.floatChannelData![0], count: Int(input.frameLength)))
    }
    guard let converter = AVAudioConverter(from: source, to: target) else { throw BenchError.audio("converter") }
    let ratio = 16_000 / source.sampleRate
    let capacity = AVAudioFrameCount(Double(input.frameLength) * ratio) + 1024
    guard let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { throw BenchError.audio("out buffer") }
    var consumed = false
    var conversionError: NSError?
    converter.convert(to: output, error: &conversionError) { _, status in
        if consumed { status.pointee = .endOfStream; return nil }
        consumed = true
        status.pointee = .haveData
        return input
    }
    if let conversionError { throw conversionError }
    return Array(UnsafeBufferPointer(start: output.floatChannelData![0], count: Int(output.frameLength)))
}

enum BenchError: Error, CustomStringConvertible {
    case audio(String)
    case engine(String)
    var description: String {
        switch self {
        case .audio(let s): return "audio: \(s)"
        case .engine(let s): return "engine: \(s)"
        }
    }
}

// MARK: - Memory

func footprintMB() -> Double {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
    let kr = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
        }
    }
    return kr == KERN_SUCCESS ? Double(info.phys_footprint) / 1_048_576 : -1
}

// MARK: - Text metrics

func normalizeWords(_ text: String) -> [String] {
    var s = text.lowercased()
    for a in ["’", "ʼ", "`", "‘"] { s = s.replacingOccurrences(of: a, with: "'") }
    let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "'"))
    let cleaned = String(s.unicodeScalars.map { allowed.contains($0) ? Character($0) : " " })
    return cleaned.split(whereSeparator: { $0 == " " }).map(String.init).filter { !$0.isEmpty }
}

func levenshtein<T: Equatable>(_ a: [T], _ b: [T]) -> Int {
    if a.isEmpty { return b.count }
    if b.isEmpty { return a.count }
    var prev = Array(0...b.count)
    var cur = [Int](repeating: 0, count: b.count + 1)
    for i in 1...a.count {
        cur[0] = i
        for j in 1...b.count {
            let cost = a[i - 1] == b[j - 1] ? 0 : 1
            cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost)
        }
        swap(&prev, &cur)
    }
    return prev[b.count]
}

func wer(reference: String, hypothesis: String) -> Double {
    let r = normalizeWords(reference), h = normalizeWords(hypothesis)
    guard !r.isEmpty else { return h.isEmpty ? 0 : 1 }
    return Double(levenshtein(r, h)) / Double(r.count)
}

func cer(reference: String, hypothesis: String) -> Double {
    let r = Array(normalizeWords(reference).joined(separator: " ")), h = Array(normalizeWords(hypothesis).joined(separator: " "))
    guard !r.isEmpty else { return h.isEmpty ? 0 : 1 }
    return Double(levenshtein(r, h)) / Double(r.count)
}

/// Latin-script tokens of the reference (identifiers, product names, commands) that survive in the hypothesis.
func latinKept(reference: String, hypothesis: String) -> (kept: Int, total: Int) {
    let latin = normalizeWords(reference).filter { $0.unicodeScalars.contains { CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyz").contains($0) } }
    guard !latin.isEmpty else { return (0, 0) }
    let hyp = Set(normalizeWords(hypothesis))
    return (latin.filter { hyp.contains($0) }.count, latin.count)
}

func scriptLabel(_ text: String) -> String {
    let scalars = text.unicodeScalars
    let cyr = scalars.filter { (0x0400...0x04FF).contains($0.value) }.count
    let lat = scalars.filter { (0x41...0x5A).contains($0.value) || (0x61...0x7A).contains($0.value) }.count
    if cyr == 0 && lat == 0 { return "-" }
    if cyr > 0 && lat > 0 { return "mixed" }
    return cyr > 0 ? "cyr" : "lat"
}

// MARK: - Engines

protocol Engine {
    var name: String { get }
    mutating func load() async throws
    func transcribe(_ samples: [Float]) async throws -> (text: String, language: String?)
}

struct ParakeetEngine: Engine {
    let name: String
    let version: AsrModelVersion
    private var manager: AsrManager?

    init(name: String, version: AsrModelVersion) {
        self.name = name
        self.version = version
    }

    mutating func load() async throws {
        let models = try await AsrModels.downloadAndLoad(version: version)
        let m = AsrManager(config: .default)
        try await m.loadModels(models)
        manager = m
    }

    func transcribe(_ samples: [Float]) async throws -> (text: String, language: String?) {
        guard let manager else { throw BenchError.engine("\(name) not loaded") }
        var state = try TdtDecoderState(decoderLayers: await manager.decoderLayerCount)
        let result = try await manager.transcribe(samples, decoderState: &state, language: nil)
        return (result.text, nil)
    }
}

struct WhisperEngine: Engine {
    let name: String
    let model: String
    private var kit: WhisperKit?

    init(name: String, model: String) {
        self.name = name
        self.model = model
    }

    mutating func load() async throws {
        let base = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/Shepit/models/whisperkit", isDirectory: true)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        let config = WhisperKitConfig(model: model, downloadBase: base, verbose: false, logLevel: .error, prewarm: true, load: true, download: true)
        kit = try await WhisperKit(config)
    }

    func transcribe(_ samples: [Float]) async throws -> (text: String, language: String?) {
        guard let kit else { throw BenchError.engine("\(name) not loaded") }
        let options = DecodingOptions(task: .transcribe, language: nil, detectLanguage: true, skipSpecialTokens: true, withoutTimestamps: true)
        let results = try await kit.transcribe(audioArray: samples, decodeOptions: options)
        let text = results.map { $0.text.trimmingCharacters(in: .whitespacesAndNewlines) }.joined(separator: " ")
        return (text, results.first?.language)
    }
}

func makeEngine(_ id: String, whisperModel: String) -> Engine? {
    switch id {
    case "parakeet-v3": return ParakeetEngine(name: id, version: .v3)
    case "parakeet-ultra": return ParakeetEngine(name: id, version: .ultra)
    case "parakeet-redux": return ParakeetEngine(name: id, version: .redux)
    case "whisper-turbo": return WhisperEngine(name: id, model: whisperModel)
    default: return nil
    }
}

// MARK: - Run

struct Clip {
    let name: String
    let url: URL
    let samples: [Float]
    let reference: String?
    var seconds: Double { Double(samples.count) / 16_000 }
    var category: String {
        if name.contains("mixed") { return "mixed" }
        if name.contains("-uk-") { return "uk" }
        if name.contains("-en-") { return "en" }
        return "other"
    }
}

struct ClipResult: Codable {
    let clip: String
    let engine: String
    let seconds: Double
    let latency: Double
    let text: String
    let language: String?
    let wer: Double?
    let cer: Double?
    let latinKept: Int?
    let latinTotal: Int?
}

struct EngineSummary: Codable {
    let engine: String
    let loadSeconds: Double
    let modelMemoryMB: Double
    let peakMemoryMB: Double
    let clips: Int
    let medianLatency: Double
    let meanLatency: Double
    let rtfx: Double
    let meanWER: [String: Double]
    let latinKeptRate: Double?
    let error: String?
}

func median(_ xs: [Double]) -> Double {
    guard !xs.isEmpty else { return 0 }
    let s = xs.sorted()
    return s.count % 2 == 1 ? s[s.count / 2] : (s[s.count / 2 - 1] + s[s.count / 2]) / 2
}

func fmt(_ x: Double, _ digits: Int = 2) -> String { String(format: "%.\(digits)f", x) }
func pct(_ x: Double?) -> String { x.map { fmt($0 * 100, 1) + "%" } ?? "-" }

@main
struct Bench {
    static func main() async {
        let opts = Options.parse(Array(CommandLine.arguments.dropFirst()))
        let fm = FileManager.default
        guard let files = try? fm.contentsOfDirectory(at: opts.recordings, includingPropertiesForKeys: nil) else {
            fail("cannot list \(opts.recordings.path)")
        }
        let wavs = files.filter { ["wav", "m4a", "mp3", "aiff", "caf", "flac"].contains($0.pathExtension.lowercased()) }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
        guard !wavs.isEmpty else { fail("no audio files in \(opts.recordings.path)") }

        var clips: [Clip] = []
        for url in wavs {
            do {
                let samples = try loadSamples16k(url)
                let name = url.deletingPathExtension().lastPathComponent
                let refURL = opts.phrases.appendingPathComponent(name + ".txt")
                var reference: String? = nil
                if let text = try? String(contentsOf: refURL, encoding: .utf8) {
                    let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
                    reference = t.hasPrefix("FREE:") ? nil : t
                }
                clips.append(Clip(name: name, url: url, samples: samples, reference: reference))
            } catch {
                log("skip \(url.lastPathComponent): \(error)")
            }
        }
        log("\(clips.count) clips, \(fmt(clips.reduce(0) { $0 + $1.seconds }, 1)) s of audio; engines: \(opts.engines.joined(separator: ", "))")

        var allResults: [ClipResult] = []
        var summaries: [EngineSummary] = []

        for id in opts.engines {
            guard var engine = makeEngine(id, whisperModel: opts.whisperModel) else {
                log("unknown engine \(id)"); continue
            }
            let before = footprintMB()
            let t0 = Date()
            do {
                try await engine.load()
            } catch {
                log("\(id): load failed: \(error)")
                summaries.append(EngineSummary(engine: id, loadSeconds: Date().timeIntervalSince(t0), modelMemoryMB: 0, peakMemoryMB: 0, clips: 0, medianLatency: 0, meanLatency: 0, rtfx: 0, meanWER: [:], latinKeptRate: nil, error: "\(error)"))
                continue
            }
            let loadSeconds = Date().timeIntervalSince(t0)
            let afterLoad = footprintMB()
            log("\(id): loaded in \(fmt(loadSeconds, 1)) s, +\(fmt(afterLoad - before, 0)) MB")

            // Warm-up on one second of silence so the first measured clip is not paying for lazy init.
            _ = try? await engine.transcribe([Float](repeating: 0, count: 16_000))

            var peak = afterLoad
            var engineResults: [ClipResult] = []
            for clip in clips {
                let t = Date()
                do {
                    let (text, language) = try await engine.transcribe(clip.samples)
                    let latency = Date().timeIntervalSince(t)
                    peak = max(peak, footprintMB())
                    var w: Double? = nil, c: Double? = nil, kept: Int? = nil, total: Int? = nil
                    if let ref = clip.reference {
                        w = wer(reference: ref, hypothesis: text)
                        c = cer(reference: ref, hypothesis: text)
                        let lk = latinKept(reference: ref, hypothesis: text)
                        if lk.total > 0 { kept = lk.kept; total = lk.total }
                    }
                    let r = ClipResult(clip: clip.name, engine: id, seconds: clip.seconds, latency: latency, text: text, language: language, wer: w, cer: c, latinKept: kept, latinTotal: total)
                    engineResults.append(r)
                    log("  \(clip.name) [\(fmt(clip.seconds, 1)) s] \(fmt(latency, 2)) s  WER \(pct(w))  \(text.prefix(90))")
                } catch {
                    log("  \(clip.name): failed: \(error)")
                }
            }
            allResults.append(contentsOf: engineResults)

            let latencies = engineResults.map(\.latency)
            let audioSeconds = engineResults.reduce(0) { $0 + $1.seconds }
            var meanWER: [String: Double] = [:]
            for cat in ["uk", "en", "mixed", "all"] {
                let rs = engineResults.filter { r in (cat == "all" || clips.first { $0.name == r.clip }?.category == cat) && r.wer != nil }
                if !rs.isEmpty { meanWER[cat] = rs.reduce(0) { $0 + $1.wer! } / Double(rs.count) }
            }
            let keptTotal = engineResults.compactMap(\.latinTotal).reduce(0, +)
            let kept = engineResults.compactMap(\.latinKept).reduce(0, +)
            summaries.append(EngineSummary(
                engine: id, loadSeconds: loadSeconds, modelMemoryMB: afterLoad - before, peakMemoryMB: peak,
                clips: engineResults.count, medianLatency: median(latencies),
                meanLatency: latencies.isEmpty ? 0 : latencies.reduce(0, +) / Double(latencies.count),
                rtfx: latencies.reduce(0, +) > 0 ? audioSeconds / latencies.reduce(0, +) : 0,
                meanWER: meanWER, latinKeptRate: keptTotal > 0 ? Double(kept) / Double(keptTotal) : nil, error: nil))
        }

        // Report
        try? fm.createDirectory(at: opts.out, withIntermediateDirectories: true)
        let stamp: String = {
            let f = DateFormatter(); f.dateFormat = "yyyyMMdd-HHmm"; return f.string(from: Date())
        }()
        var md = "# shepit-bench \(stamp)\n\n"
        md += "Machine: \(machineDescription())\n\n"
        md += "| Engine | Load s | Model MB | Peak MB | Clips | Median s | Mean s | RTFx | WER uk | WER en | WER mixed | WER all | Latin kept |\n"
        md += "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n"
        for s in summaries {
            if let e = s.error { md += "| \(s.engine) | load failed: \(e) |\n"; continue }
            md += "| \(s.engine) | \(fmt(s.loadSeconds, 1)) | \(fmt(s.modelMemoryMB, 0)) | \(fmt(s.peakMemoryMB, 0)) | \(s.clips) | \(fmt(s.medianLatency)) | \(fmt(s.meanLatency)) | \(fmt(s.rtfx, 0)) | \(pct(s.meanWER["uk"])) | \(pct(s.meanWER["en"])) | \(pct(s.meanWER["mixed"])) | \(pct(s.meanWER["all"])) | \(pct(s.latinKeptRate)) |\n"
        }
        md += "\n## Per clip\n"
        for clip in clips {
            md += "\n### \(clip.name) (\(fmt(clip.seconds, 1)) s, \(clip.category))\n\n"
            if let ref = clip.reference { md += "Reference: \(ref)\n\n" } else { md += "Reference: none (free speech)\n\n" }
            md += "| Engine | Latency s | WER | CER | Latin kept | Lang | Script | Text |\n| --- | ---: | ---: | ---: | ---: | --- | --- | --- |\n"
            for r in allResults.filter({ $0.clip == clip.name }) {
                let lk = r.latinTotal.map { "\(r.latinKept ?? 0)/\($0)" } ?? "-"
                md += "| \(r.engine) | \(fmt(r.latency)) | \(pct(r.wer)) | \(pct(r.cer)) | \(lk) | \(r.language ?? "-") | \(scriptLabel(r.text)) | \(r.text.replacingOccurrences(of: "|", with: "\\|").replacingOccurrences(of: "\n", with: " ")) |\n"
            }
        }
        let mdURL = opts.out.appendingPathComponent("bench-\(stamp).md")
        let jsonURL = opts.out.appendingPathComponent("bench-\(stamp).json")
        try? md.write(to: mdURL, atomically: true, encoding: .utf8)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        struct Report: Codable { let summaries: [EngineSummary]; let results: [ClipResult] }
        if let data = try? encoder.encode(Report(summaries: summaries, results: allResults)) { try? data.write(to: jsonURL) }
        print(md.components(separatedBy: "\n## Per clip").first ?? md)
        print("report: \(mdURL.path)")
    }

    static func machineDescription() -> String {
        var size = 0
        sysctlbyname("machdep.cpu.brand_string", nil, &size, nil, 0)
        var buf = [CChar](repeating: 0, count: size)
        sysctlbyname("machdep.cpu.brand_string", &buf, &size, nil, 0)
        let cpu = String(cString: buf)
        let mem = ProcessInfo.processInfo.physicalMemory / 1_073_741_824
        let os = ProcessInfo.processInfo.operatingSystemVersionString
        return "\(cpu), \(mem) GB, \(os)"
    }
}
