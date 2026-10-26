import AVFoundation
import Foundation

public enum AudioError: Error, LocalizedError {
    case noInput
    case permissionDenied
    case formatMismatch(tap: Double, hardware: Double)

    public var errorDescription: String? {
        switch self {
        case .noInput: return "No audio input device"
        case .permissionDenied: return "Microphone access was denied"
        case .formatMismatch(let tap, let hardware):
            return "Input format changed (\(Int(tap)) Hz vs \(Int(hardware)) Hz), try again"
        }
    }
}

/// Collects converted 16 kHz mono samples from the audio thread. Thread-safe.
final class CaptureBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var samples: [Float] = []
    private var converter: AVAudioConverter?
    private let targetFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!

    func reset(inputFormat: AVAudioFormat) {
        lock.lock(); defer { lock.unlock() }
        samples.removeAll(keepingCapacity: true)
        converter = AVAudioConverter(from: inputFormat, to: targetFormat)
    }

    /// Converts and appends one buffer. Returns the RMS level of the converted audio in 0...1.
    func append(_ buffer: AVAudioPCMBuffer) -> Float {
        lock.lock(); defer { lock.unlock() }
        guard let converter else { return 0 }
        let ratio = 16_000 / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
        guard let out = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else { return 0 }
        var consumed = false
        var error: NSError?
        converter.convert(to: out, error: &error) { _, status in
            if consumed { status.pointee = .noDataNow; return nil }
            consumed = true
            status.pointee = .haveData
            return buffer
        }
        guard error == nil, let channel = out.floatChannelData?[0] else { return 0 }
        let count = Int(out.frameLength)
        guard count > 0 else { return 0 }
        samples.append(contentsOf: UnsafeBufferPointer(start: channel, count: count))
        var sum: Float = 0
        for i in 0..<count { sum += channel[i] * channel[i] }
        let rms = (sum / Float(count)).squareRoot()
        let db = 20 * log10(max(rms, 1e-6))
        return min(1, max(0, (db + 50) / 50))
    }

    func drain() -> [Float] {
        lock.lock(); defer { lock.unlock() }
        let out = samples
        samples = []
        return out
    }

    var seconds: Double {
        lock.lock(); defer { lock.unlock() }
        return Double(samples.count) / 16_000
    }

    var count: Int {
        lock.lock(); defer { lock.unlock() }
        return samples.count
    }

    func copy(from start: Int) -> [Float] {
        lock.lock(); defer { lock.unlock() }
        guard start < samples.count else { return [] }
        return Array(samples[max(0, start)...])
    }
}

/// Microphone capture to 16 kHz mono Float32, with a smoothed level for the widget.
///
/// The engine stays prepared between dictations so a push-to-talk start loses no audio. Its input
/// node caches the device format it saw when prepared, and installing a tap with a format that no
/// longer matches the hardware (AirPods at 24 kHz connected or removed since then) raises an
/// Objective-C exception Swift cannot catch. The recorder rebuilds the engine after a configuration
/// change and whenever the cached format disagrees with the hardware format.
@MainActor
public final class AudioRecorder: ObservableObject, LiveAudioSource {
    @Published public private(set) var level: Float = 0
    @Published public private(set) var isRecording = false

    private var engine = AVAudioEngine()
    private let capture = CaptureBuffer()
    private var tapInstalled = false
    private var needsRebuild = false
    private var configObserver: NSObjectProtocol?

    public init() {
        observeConfigurationChanges()
    }

    deinit {
        if let configObserver { NotificationCenter.default.removeObserver(configObserver) }
    }

    public static var permissionGranted: Bool {
        AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
    }

    public static func requestPermission() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return true
        case .notDetermined: return await AVCaptureDevice.requestAccess(for: .audio)
        default: return false
        }
    }

    /// Touches the input node so the first real start is faster.
    public func prepare() {
        if needsRebuild { rebuildEngine() }
        _ = engine.inputNode
        engine.prepare()
    }

    public func start() throws {
        guard !isRecording else { return }
        if needsRebuild { rebuildEngine() }
        var input = engine.inputNode
        var format = input.outputFormat(forBus: 0)
        var hardware = input.inputFormat(forBus: 0)
        if !Self.sameShape(format, hardware) {
            Log.audio.notice("cached input format \(format.sampleRate, privacy: .public) Hz differs from hardware \(hardware.sampleRate, privacy: .public) Hz, rebuilding engine")
            rebuildEngine()
            input = engine.inputNode
            format = input.outputFormat(forBus: 0)
            hardware = input.inputFormat(forBus: 0)
        }
        guard format.sampleRate > 0, format.channelCount > 0, hardware.sampleRate > 0 else { throw AudioError.noInput }
        // AVAudioEngine asserts on this mismatch inside installTap; fail with an error instead.
        guard format.sampleRate == hardware.sampleRate else {
            needsRebuild = true
            throw AudioError.formatMismatch(tap: format.sampleRate, hardware: hardware.sampleRate)
        }
        capture.reset(inputFormat: format)
        if tapInstalled { input.removeTap(onBus: 0) }
        let capture = self.capture
        input.installTap(onBus: 0, bufferSize: 2048, format: format) { [weak self] buffer, _ in
            let level = capture.append(buffer)
            Task { @MainActor in self?.updateLevel(level) }
        }
        tapInstalled = true
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            tapInstalled = false
            needsRebuild = true
            throw error
        }
        isRecording = true
        Log.audio.notice("capture started at \(format.sampleRate, privacy: .public) Hz, \(format.channelCount, privacy: .public) ch")
    }

    /// Stops capture and returns everything recorded since `start()`.
    public func stop() -> [Float] {
        guard isRecording else { return [] }
        engine.inputNode.removeTap(onBus: 0)
        tapInstalled = false
        engine.stop()
        isRecording = false
        level = 0
        let samples = capture.drain()
        Log.audio.notice("capture stopped, \(Double(samples.count) / 16_000, format: .fixed(precision: 2)) s")
        return samples
    }

    public var recordedSeconds: Double { capture.seconds }

    public var capturedSampleCount: Int { capture.count }

    public func capturedSamples(from start: Int) -> [Float] { capture.copy(from: start) }

    private func updateLevel(_ new: Float) {
        level = level * 0.6 + new * 0.4
    }

    private static func sameShape(_ a: AVAudioFormat, _ b: AVAudioFormat) -> Bool {
        a.sampleRate == b.sampleRate && a.channelCount == b.channelCount
    }

    /// Replaces the engine so its input node picks up the current device format.
    private func rebuildEngine() {
        if tapInstalled { engine.inputNode.removeTap(onBus: 0) }
        tapInstalled = false
        engine.stop()
        engine = AVAudioEngine()
        needsRebuild = false
        observeConfigurationChanges()
        _ = engine.inputNode
        engine.prepare()
    }

    private func observeConfigurationChanges() {
        if let configObserver { NotificationCenter.default.removeObserver(configObserver) }
        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.handleConfigurationChange() }
        }
    }

    private func handleConfigurationChange() {
        needsRebuild = true
        if isRecording {
            Log.audio.error("audio device changed during capture; the recording keeps what was captured so far")
        } else {
            Log.audio.notice("audio configuration changed, engine rebuilds before the next capture")
        }
    }
}
