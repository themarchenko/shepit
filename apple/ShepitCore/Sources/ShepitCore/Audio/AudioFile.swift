import AVFoundation
import Foundation

public enum AudioFile {
    /// Loads any audio file AVFoundation can read and returns 16 kHz mono Float32 samples.
    public static func loadSamples16k(_ url: URL) throws -> [Float] {
        let file = try AVAudioFile(forReading: url)
        let source = file.processingFormat
        let frameCount = AVAudioFrameCount(file.length)
        guard let input = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: max(frameCount, 1)) else {
            throw AudioError.noInput
        }
        try file.read(into: input)
        let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
        if source.sampleRate == 16_000, source.channelCount == 1, source.commonFormat == .pcmFormatFloat32,
           let channel = input.floatChannelData?[0] {
            return Array(UnsafeBufferPointer(start: channel, count: Int(input.frameLength)))
        }
        guard let converter = AVAudioConverter(from: source, to: target) else { throw AudioError.noInput }
        let ratio = 16_000 / source.sampleRate
        let capacity = AVAudioFrameCount(Double(input.frameLength) * ratio) + 1024
        guard let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { throw AudioError.noInput }
        var consumed = false
        var conversionError: NSError?
        converter.convert(to: output, error: &conversionError) { _, status in
            if consumed {
                status.pointee = .endOfStream
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return input
        }
        if let conversionError { throw conversionError }
        guard let channel = output.floatChannelData?[0] else { return [] }
        return Array(UnsafeBufferPointer(start: channel, count: Int(output.frameLength)))
    }
}
