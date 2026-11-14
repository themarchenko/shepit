import FluidAudio
import Foundation

public extension EngineID {
    /// Size of a complete model folder, measured on a finished download (2026-11-14). Neither library reports
    /// bytes: WhisperKit counts files and FluidAudio gives a fraction, so progress is read from the disk.
    var downloadBytes: Int64 {
        switch self {
        case .parakeetV3: return 483_257_242
        case .parakeetUltra: return 632_314_500
        case .whisperTurbo: return 1_641_232_216
        }
    }

    /// The folder the engine downloads into. Partial files (`.partial`, `.incomplete`) are written inside it.
    var modelDirectory: URL {
        switch self {
        case .parakeetV3: return AsrModels.defaultCacheDirectory(for: .v3)
        case .parakeetUltra: return AsrModels.defaultCacheDirectory(for: .ultra)
        case .whisperTurbo: return AppPaths.modelsDirectory.appendingPathComponent("whisperkit", isDirectory: true)
        }
    }
}

public enum ModelFolder {
    /// Total size of the regular files under `url`, partial downloads included.
    public static func bytes(at url: URL) -> Int64 {
        guard let walker = FileManager.default.enumerator(at: url, includingPropertiesForKeys: [.fileSizeKey, .isRegularFileKey]) else { return 0 }
        var total: Int64 = 0
        for case let file as URL in walker {
            guard let values = try? file.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey]),
                  values.isRegularFile == true else { continue }
            total += Int64(values.fileSize ?? 0)
        }
        return total
    }
}

/// Download speed and time left from periodic byte counts.
public struct DownloadMeter: Sendable {
    public struct Reading: Equatable, Sendable {
        public var bytes: Int64
        public var total: Int64
        public var bytesPerSecond: Double?
        public var secondsLeft: Double?

        public init(bytes: Int64, total: Int64, bytesPerSecond: Double? = nil, secondsLeft: Double? = nil) {
            self.bytes = bytes
            self.total = total
            self.bytesPerSecond = bytesPerSecond
            self.secondsLeft = secondsLeft
        }

        /// Never 1 while downloading: the expected size is a measurement, not a promise.
        public var fraction: Double { total > 0 ? min(0.99, max(0, Double(bytes) / Double(total))) : 0 }
    }

    /// Speed is averaged over this many seconds, so one slow second does not swing the estimate.
    static let window: TimeInterval = 10
    /// Shorter spans give a speed too noisy to show.
    static let minimumSpan: TimeInterval = 2

    public let total: Int64
    private var samples: [(at: Date, bytes: Int64)] = []

    public init(total: Int64) { self.total = total }

    public mutating func add(bytes: Int64, at now: Date) -> Reading {
        // A restarted download (bytes went down) starts the average over.
        if let last = samples.last, bytes < last.bytes { samples.removeAll() }
        samples.append((now, bytes))
        samples.removeAll { now.timeIntervalSince($0.at) > Self.window }
        var reading = Reading(bytes: bytes, total: total)
        if let first = samples.first, now.timeIntervalSince(first.at) >= Self.minimumSpan {
            let rate = Double(bytes - first.bytes) / now.timeIntervalSince(first.at)
            if rate > 0 {
                reading.bytesPerSecond = rate
                reading.secondsLeft = Double(max(0, total - bytes)) / rate
            }
        }
        return reading
    }
}

/// What the dictation key can rely on right now. Kept on the main actor so a key press reads it without a hop
/// to the engine manager.
@MainActor
public final class ModelStatus: ObservableObject {
    public enum Phase: Equatable, Sendable {
        case ready
        /// Not loaded and nothing is loading it.
        case idle
        case downloading(DownloadMeter.Reading)
        /// Files are on disk; the model is being compiled or loaded into memory.
        case preparing
        case failed(String)
    }

    @Published public private(set) var phase: Phase = .idle
    /// The engine `phase` describes. Right after the setting changes it is still the old one.
    public private(set) var engine: EngineID?

    /// Whether a dictation started now gets the chosen engine without waiting for a download or a load.
    public var isReady: Bool { phase == .ready && engine == engineID() }

    /// Below this share of the expected size a loading model counts as downloading. The margin covers small
    /// differences between model revisions.
    static let downloadedShare = 0.97

    private let engines: EngineManager
    private let engineID: @MainActor () -> EngineID
    private let measure: @Sendable (EngineID) -> Int64
    private var meter: (id: EngineID, meter: DownloadMeter)?
    private var timer: Timer?
    private var generation = 0

    public init(engines: EngineManager, engineID: @escaping @MainActor () -> EngineID,
                measure: @escaping @Sendable (EngineID) -> Int64 = { ModelFolder.bytes(at: $0.modelDirectory) }) {
        self.engines = engines
        self.engineID = engineID
        self.measure = measure
    }

    /// Polls once a second for download progress, and right away whenever the engine manager changes state.
    public func start() {
        guard timer == nil else { return }
        Task {
            await engines.setOnChange { [weak self] in
                Task { @MainActor in await self?.refresh() }
            }
        }
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                Task { await self.refresh() }
            }
        }
        Task { await refresh() }
    }

    public func refresh() async {
        generation += 1
        let mine = generation
        let id = engineID()
        let state = await engines.loadState(of: id)
        var bytes: Int64 = 0
        if state == .loading {
            let measure = self.measure
            bytes = await Task.detached(priority: .utility) { measure(id) }.value
        }
        // A slower, older refresh must not overwrite a newer one.
        guard mine == generation else { return }
        let next = phase(for: state, id: id, bytes: bytes, now: Date())
        engine = id
        if next != phase { phase = next }
    }

    func phase(for state: EngineLoadState, id: EngineID, bytes: Int64, now: Date) -> Phase {
        switch state {
        case .loaded:
            meter = nil
            return .ready
        case .failed(let message):
            meter = nil
            return .failed(message)
        case .idle:
            meter = nil
            return .idle
        case .loading:
            guard Double(bytes) < Double(id.downloadBytes) * Self.downloadedShare else { return .preparing }
            if meter?.id != id { meter = (id, DownloadMeter(total: id.downloadBytes)) }
            return .downloading(meter!.meter.add(bytes: bytes, at: now))
        }
    }

    /// Why the dictation key does nothing right now, or nil when the model is ready.
    public var notReadyMessage: String? {
        isReady ? nil : Self.notReadyMessage(phase)
    }

    /// Short: the widget shows it on one line. `.ready` here means ready for an engine that is no longer chosen.
    nonisolated public static func notReadyMessage(_ phase: Phase) -> String {
        switch phase {
        case .downloading(let r): return L("Model downloading: \(percent(r.fraction))")
        case .ready, .idle, .preparing: return L("Model is still loading")
        case .failed: return L("Model did not load")
        }
    }

    nonisolated public static func percent(_ fraction: Double) -> String {
        let f = NumberFormatter()
        f.numberStyle = .percent
        f.maximumFractionDigits = 0
        f.locale = Locale(identifier: CoreLocalization.language)
        return f.string(from: NSNumber(value: fraction)) ?? "\(Int(fraction * 100))%"
    }

    /// "640 MB of 1.6 GB, 12 MB/s, about 2 min left" in the core language.
    nonisolated public static func progressText(_ r: DownloadMeter.Reading) -> String {
        // In the interface language, not the system region, so the numbers match the words around them.
        let style = ByteCountFormatStyle(style: .file, allowedUnits: .all, spellsOutZero: false,
                                         includesActualByteCount: false, locale: Locale(identifier: CoreLocalization.language))
        var text = L("\(style.format(r.bytes)) of \(style.format(r.total))")
        if let rate = r.bytesPerSecond {
            text += ", " + L("\(style.format(Int64(rate)))/s")
        }
        if let left = r.secondsLeft {
            let minutes = Int((left / 60).rounded(.up))
            text += ", " + (left < 60 ? L("less than a minute left") : L("about \(minutes) min left"))
        }
        return text
    }
}
