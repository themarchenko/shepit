//! Capture maths: downmix, level meter, level history and the buffer that turns device frames
//! into 16 kHz mono. No device access here, so it all runs in tests.

use crate::resample::Resampler;

/// Levels are computed per 2048 device frames (AudioRecorder.swift:149).
pub const LEVEL_FRAMES: usize = 2048;
/// The widget keeps the newest 32 levels (WidgetModel.swift:34).
pub const LEVEL_HISTORY_LEN: usize = 32;
/// Recordings shorter than 0.4 s are dropped (DictationSession.swift:49).
pub const MIN_SAMPLES: usize = 6400;

/// Averages the channels of interleaved frames. A trailing partial frame is dropped.
pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|f| f.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// 0..1 meter value from a block of samples (AudioRecorder.swift:51-55).
pub fn level_from_samples(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_sq = samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32;
    // Swift (AudioRecorder.swift:54-55): db = 20*log10(max(rms, 1e-6)), clamp((db + 50) / 50, 0, 1).
    let db = 20.0 * mean_sq.sqrt().max(1e-6).log10();
    ((db + 50.0) / 50.0).clamp(0.0, 1.0)
}

/// Smoothing applied to each new level (AudioRecorder.swift:186).
pub fn smooth_level(previous: f32, new: f32) -> f32 {
    // Swift (AudioRecorder.swift:186): level * 0.6 + new * 0.4.
    previous * 0.6 + new * 0.4
}

pub fn is_long_enough(samples: usize) -> bool {
    samples >= MIN_SAMPLES
}

/// True for no samples, or when every sample is within 1e-6 of zero: a muted or blocked
/// microphone delivers exact zeros, while a quiet room still has noise above that.
pub fn is_digital_silence(samples: &[f32]) -> bool {
    samples.iter().all(|s| s.abs() <= 1e-6)
}

/// The last `LEVEL_HISTORY_LEN` levels, oldest first, zero-filled until full.
#[derive(Debug, Clone)]
pub struct LevelHistory {
    levels: Vec<f32>,
}

impl LevelHistory {
    pub fn new() -> Self {
        Self {
            levels: vec![0.0; LEVEL_HISTORY_LEN],
        }
    }

    pub fn push(&mut self, level: f32) {
        self.levels.remove(0);
        self.levels.push(level);
    }

    pub fn levels(&self) -> &[f32] {
        &self.levels
    }

    pub fn clear(&mut self) {
        self.levels.fill(0.0);
    }
}

impl Default for LevelHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Collects device frames as 16 kHz mono and reports one raw level per 2048 device frames.
pub struct CaptureBuffer {
    rate: u32,
    channels: usize,
    resampler: Resampler,
    samples: Vec<f32>,
    /// Mono device-rate samples not yet part of a full 2048-frame level window.
    level_window: Vec<f32>,
    device_frames: usize,
}

impl CaptureBuffer {
    pub fn new(rate: u32, channels: usize) -> Result<Self, String> {
        Ok(Self {
            rate,
            channels: channels.max(1),
            resampler: Resampler::new(rate)?,
            samples: Vec::new(),
            level_window: Vec::new(),
            device_frames: 0,
        })
    }

    /// Takes interleaved device frames. Returns the raw levels completed by this chunk.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let mono = downmix(interleaved, self.channels);
        self.device_frames += mono.len();
        self.samples.extend(self.resampler.process(&mono));
        self.level_window.extend_from_slice(&mono);
        let mut levels = Vec::new();
        while self.level_window.len() >= LEVEL_FRAMES {
            levels.push(level_from_samples(&self.level_window[..LEVEL_FRAMES]));
            self.level_window.drain(..LEVEL_FRAMES);
        }
        levels
    }

    /// Flushes the resampler and returns the whole recording, trimmed to floor(frames*16000/rate).
    pub fn finish(&mut self) -> Vec<f32> {
        self.samples.extend(self.resampler.finish());
        let target = (self.device_frames as u64 * 16_000 / self.rate as u64) as usize;
        self.samples.truncate(target);
        std::mem::take(&mut self.samples)
    }
}

/// A canonical RIFF/WAVE file: 16 kHz, mono, 16-bit PCM, a 44-byte header (RecordingStore.swift:45-74).
/// Samples are clamped to -1..=1 and scaled by 32767, halves rounded away from zero like Swift's
/// `.rounded()`.
pub fn wav_16k_mono(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        // f32::round is half away from zero, like Swift's `.rounded()`.
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn stereo_averages_to_mono() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.25], 2), vec![0.5, 0.375]);
    }

    #[test]
    fn mono_passes_through() {
        assert_eq!(downmix(&[0.1, -0.2, 0.3], 1), vec![0.1, -0.2, 0.3]);
    }

    #[test]
    fn trailing_partial_frame_is_dropped() {
        assert_eq!(downmix(&[1.0, 1.0, 0.5, 0.5, 9.0], 2), vec![1.0, 0.5]);
    }

    #[test]
    fn silence_has_level_zero() {
        assert_eq!(level_from_samples(&[0.0; 512]), 0.0);
    }

    #[test]
    fn full_scale_has_level_one() {
        assert_eq!(level_from_samples(&[1.0; 512]), 1.0);
    }

    #[test]
    fn minus_6_dbfs_sine_has_level_0_819() {
        // Peak 0.5 is -6 dBFS; its RMS is -9.03 dB, so (50 - 9.03) / 50 = 0.819.
        let s: Vec<f32> = (0..16_000)
            .map(|i| 0.5 * (2.0 * PI * 1000.0 * i as f32 / 16_000.0).sin())
            .collect();
        let l = level_from_samples(&s);
        assert!((l - 0.819).abs() < 0.005, "level {l}");
    }

    #[test]
    fn smoothing_climbs_towards_the_new_level() {
        let a = smooth_level(0.0, 1.0);
        let b = smooth_level(a, 1.0);
        assert!((a - 0.4).abs() < 1e-6, "{a}");
        assert!((b - 0.64).abs() < 1e-6, "{b}");
    }

    #[test]
    fn history_holds_32_levels_and_drops_the_oldest() {
        let mut h = LevelHistory::new();
        for i in 0..40 {
            h.push(i as f32 / 100.0);
        }
        assert_eq!(h.levels().len(), 32);
        assert!((h.levels()[0] - 0.08).abs() < 1e-6);
        assert!((h.levels()[31] - 0.39).abs() < 1e-6);
    }

    #[test]
    fn history_starts_zero_filled_and_clears_to_zero() {
        let mut h = LevelHistory::new();
        assert_eq!(h.levels(), &[0.0; 32][..]);
        h.push(0.7);
        h.clear();
        assert_eq!(h.levels(), &[0.0; 32][..]);
    }

    #[test]
    fn recording_length_threshold_is_0_4_seconds() {
        assert!(is_long_enough(6400));
        assert!(!is_long_enough(6399));
    }

    fn feed(buf: &mut CaptureBuffer, frames: usize, chunk: usize, channels: usize) -> usize {
        let mut levels = 0;
        let mut left = frames;
        while left > 0 {
            let n = chunk.min(left);
            levels += buf.push(&vec![0.25; n * channels]).len();
            left -= n;
        }
        levels
    }

    #[test]
    fn one_second_at_48k_gives_23_levels() {
        let mut buf = CaptureBuffer::new(48_000, 2).unwrap();
        assert_eq!(feed(&mut buf, 48_000, 480, 2), 23);
    }

    #[test]
    fn capture_is_trimmed_to_floor_of_the_scaled_frame_count() {
        // 19199 * 16000 / 48000 = 6399.67
        let mut buf = CaptureBuffer::new(48_000, 1).unwrap();
        feed(&mut buf, 19_199, 19_199, 1);
        assert_eq!(buf.finish().len(), 6399);
    }

    #[test]
    fn no_samples_is_digital_silence() {
        assert!(is_digital_silence(&[]));
    }

    #[test]
    fn exact_zeros_are_digital_silence() {
        assert!(is_digital_silence(&[0.0; 16_000]));
        assert!(is_digital_silence(&[-0.0, 0.0]));
    }

    #[test]
    fn samples_up_to_1e_minus_6_in_magnitude_are_still_silence() {
        assert!(is_digital_silence(&[1e-6, -1e-6, 0.5e-6]));
    }

    #[test]
    fn a_sample_above_1e_minus_6_is_not_silence() {
        assert!(!is_digital_silence(&[0.0, 0.0, 2e-6]));
        assert!(!is_digital_silence(&[-2e-6]));
    }

    #[test]
    fn quiet_noise_is_not_silence() {
        assert!(!is_digital_silence(&[1e-4; 512]));
    }

    #[test]
    fn one_loud_sample_among_zeros_is_not_silence() {
        let mut v = vec![0.0; 16_000];
        v[9_000] = 0.3;
        assert!(!is_digital_silence(&v));
    }

    fn u16_at(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes([b[i], b[i + 1]])
    }
    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }
    fn pcm(wav: &[u8]) -> Vec<i16> {
        wav[44..]
            .chunks(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    #[test]
    fn the_wav_header_is_16k_mono_16_bit_pcm() {
        let wav = wav_16k_mono(&[0.0; 3]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32_at(&wav, 4), 36 + 6);
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u32_at(&wav, 16), 16);
        assert_eq!(u16_at(&wav, 20), 1, "PCM");
        assert_eq!(u16_at(&wav, 22), 1, "mono");
        assert_eq!(u32_at(&wav, 24), 16_000);
        assert_eq!(u32_at(&wav, 28), 32_000, "byte rate");
        assert_eq!(u16_at(&wav, 32), 2, "block align");
        assert_eq!(u16_at(&wav, 34), 16, "bits");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32_at(&wav, 40), 6);
    }

    #[test]
    fn the_wav_is_44_bytes_plus_two_per_sample() {
        assert_eq!(wav_16k_mono(&[]).len(), 44);
        assert_eq!(wav_16k_mono(&[0.1; 1000]).len(), 44 + 2000);
    }

    #[test]
    fn samples_are_clamped_and_scaled_by_32767() {
        let wav = wav_16k_mono(&[2.0, -2.0, 1.0, -1.0, 0.0]);
        assert_eq!(pcm(&wav), [32767, -32767, 32767, -32767, 0]);
    }

    #[test]
    fn a_half_rounds_away_from_zero() {
        // 0.5 * 32767 = 16383.5 -> 16384; -0.5 -> -16384.
        assert_eq!(pcm(&wav_16k_mono(&[0.5, -0.5])), [16384, -16384]);
    }
}
