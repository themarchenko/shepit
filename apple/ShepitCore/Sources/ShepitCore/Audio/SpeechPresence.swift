import Foundation

/// A cheap energy-based check for whether a recording contains speech at all. It gates the
/// recognizer so that a key press with silence does not become a hallucinated sentence.
public enum SpeechPresence {
    /// Seconds of frames whose energy stands clearly above the recording's noise floor.
    public static func speechSeconds(in samples: [Float], sampleRate: Int = 16_000) -> Double {
        let frame = sampleRate / 50 // 20 ms
        guard samples.count >= frame else { return 0 }
        var energies: [Float] = []
        energies.reserveCapacity(samples.count / frame)
        var i = 0
        while i + frame <= samples.count {
            var sum: Float = 0
            for j in i..<(i + frame) { sum += samples[j] * samples[j] }
            energies.append((sum / Float(frame)).squareRoot())
            i += frame
        }
        let sorted = energies.sorted()
        let noiseFloor = sorted[sorted.count / 5]
        let threshold = max(noiseFloor * 3, 0.006)
        let speechFrames = energies.filter { $0 > threshold }.count
        return Double(speechFrames) * Double(frame) / Double(sampleRate)
    }
}

/// Phrases Whisper produces on silence or noise instead of admitting there was no speech.
public enum HallucinationFilter {
    static let wholeText: Set<String> = [
        "дякую за перегляд", "дякую за увагу", "продовження слідує", "до зустрічі", "субтитри",
        "thank you for watching", "thanks for watching", "thank you", "bye",
    ]
    static let fragments: [String] = [
        "субтитри створив", "субтитри зробив", "dimatorzok", "amara.org", "subtitles by", "підпишіться на канал",
    ]

    public static func isJunk(_ text: String) -> Bool {
        let lowered = text.lowercased()
        if fragments.contains(where: { lowered.contains($0) }) { return true }
        let stripped = lowered.unicodeScalars.filter { CharacterSet.letters.contains($0) || $0 == " " }
        let normalized = String(String.UnicodeScalarView(stripped)).split(separator: " ").joined(separator: " ")
        return wholeText.contains(normalized)
    }
}
