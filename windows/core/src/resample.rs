//! Streaming resampler to 16 kHz mono. FFT fixed-input (rubato), pass-through at 16 kHz.
//! Linear interpolation was rejected: it aliases content above 8 kHz into the speech band.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler as _};

pub const TARGET_RATE: u32 = 16_000;

/// Input frames per FFT call. With rubato's default sub-chunking this is four 256-frame FFT
/// blocks, so the filter delay stays near 130 output samples (8 ms) while the cutoff stays high.
const CHUNK_FRAMES: usize = 1024;

/// Converts mono f32 at `rate_in` to 16 kHz. Feed chunks of any size to `process`, then call
/// `finish` once: it flushes the filter tail and trims to floor(frames * 16000 / rate_in).
pub struct Resampler {
    rate_in: u32,
    frames_in: usize,
    /// None at 16 kHz: samples pass straight through.
    fft: Option<Fft<f32>>,
    pending: Vec<f32>,
    /// Leading output frames still to drop (the filter delay shows up as silence).
    delay_left: usize,
    /// Output frames returned so far, after the delay was dropped.
    produced: usize,
}

impl Resampler {
    pub fn new(rate_in: u32) -> Result<Self, String> {
        let (fft, delay_left) = if rate_in == TARGET_RATE {
            (None, 0)
        } else {
            let fft = Fft::<f32>::new(
                rate_in as usize,
                TARGET_RATE as usize,
                CHUNK_FRAMES,
                1,
                FixedSync::Input,
            )
            .map_err(|e| e.to_string())?;
            let delay = fft.output_delay();
            (Some(fft), delay)
        };
        Ok(Self {
            rate_in,
            frames_in: 0,
            fft,
            pending: Vec::new(),
            delay_left,
            produced: 0,
        })
    }

    /// Returns the output samples that are ready. May return fewer than a proportional share
    /// while the resampler waits for a full input block.
    pub fn process(&mut self, mono: &[f32]) -> Vec<f32> {
        self.frames_in += mono.len();
        if self.fft.is_none() {
            return mono.to_vec();
        }
        self.pending.extend_from_slice(mono);
        let mut out = Vec::new();
        loop {
            let need = self
                .fft
                .as_ref()
                .map_or(usize::MAX, |f| f.input_frames_next());
            if self.pending.len() < need {
                break;
            }
            let chunk: Vec<f32> = self.pending.drain(..need).collect();
            self.run_chunk(&chunk, None, &mut out);
        }
        self.produced += out.len();
        out
    }

    /// Flushes with zeros, compensates the filter delay and trims to the exact length.
    pub fn finish(&mut self) -> Vec<f32> {
        if self.fft.is_none() {
            return Vec::new();
        }
        let target = (self.frames_in as u64 * TARGET_RATE as u64 / self.rate_in as u64) as usize;
        let owed = target.saturating_sub(self.produced);
        let mut out = Vec::new();
        // The first flush chunk carries the real leftover frames; rubato pads the rest with
        // silence. Later chunks are pure silence that pushes the filter tail out.
        let mut leftover = Some(std::mem::take(&mut self.pending));
        while out.len() < owed {
            let chunk = leftover.take().unwrap_or_default();
            let partial = Some(chunk.len());
            if !self.run_chunk(&chunk, partial, &mut out) {
                break;
            }
        }
        out.truncate(owed);
        self.produced += out.len();
        out
    }

    fn run_chunk(&mut self, chunk: &[f32], partial: Option<usize>, out: &mut Vec<f32>) -> bool {
        let Some(fft) = self.fft.as_mut() else {
            return false;
        };
        let need = fft.input_frames_next();
        let mut input = chunk.to_vec();
        input.resize(need, 0.0);
        let Ok(input) = InterleavedSlice::new(&input, 1, need) else {
            return false;
        };
        let indexing = Indexing::new().partial_len(partial.unwrap_or(need));
        let Ok(result) = fft.process(&input, Some(&indexing)) else {
            return false;
        };
        let samples = result.take_data();
        let skip = self.delay_left.min(samples.len());
        self.delay_left -= skip;
        out.extend_from_slice(&samples[skip..]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32, rate: u32, secs: f32, amp: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn crossings(x: &[f32]) -> usize {
        x.windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count()
    }

    fn run(rate: u32, input: &[f32]) -> Vec<f32> {
        let mut r = Resampler::new(rate).unwrap();
        let mut out = r.process(input);
        out.extend(r.finish());
        out
    }

    #[test]
    fn sixteen_khz_passes_through_unchanged() {
        let input = sine(300.0, 16_000, 0.5, 0.5);
        assert_eq!(run(16_000, &input), input);
    }

    #[test]
    fn one_second_at_48k_gives_16000_samples() {
        assert_eq!(run(48_000, &sine(440.0, 48_000, 1.0, 0.5)).len(), 16_000);
    }

    #[test]
    fn one_second_at_44k1_gives_16000_samples() {
        assert_eq!(run(44_100, &sine(440.0, 44_100, 1.0, 0.5)).len(), 16_000);
    }

    #[test]
    fn tone_keeps_its_frequency() {
        let out = run(48_000, &sine(440.0, 48_000, 1.0, 0.5));
        let c = crossings(&out) as i64;
        assert!((c - 880).abs() <= 4, "zero crossings {c}");
    }

    #[test]
    fn tone_keeps_its_level() {
        let out = run(48_000, &sine(440.0, 48_000, 1.0, 0.5));
        let expected = 0.5 / 2f32.sqrt();
        assert!(
            (rms(&out) / expected - 1.0).abs() < 0.03,
            "rms {}",
            rms(&out)
        );
    }

    #[test]
    fn chunking_does_not_change_the_output() {
        let input = sine(440.0, 48_000, 1.0, 0.5);
        let whole = run(48_000, &input);
        let mut r = Resampler::new(48_000).unwrap();
        let mut out = Vec::new();
        let mut seed = 12345u32;
        let mut pos = 0;
        while pos < input.len() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let len = (1 + (seed >> 16) as usize % 1500).min(input.len() - pos);
            out.extend(r.process(&input[pos..pos + len]));
            pos += len;
        }
        out.extend(r.finish());
        assert_eq!(out.len(), whole.len());
        let worst = out
            .iter()
            .zip(&whole)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 1e-5, "max diff {worst}");
    }

    #[test]
    fn content_above_8k_is_filtered_out() {
        let input = sine(12_000.0, 48_000, 1.0, 0.5);
        let out = run(48_000, &input);
        assert!(rms(&out) < 0.05 * rms(&input), "rms {}", rms(&out));
    }

    #[test]
    fn step_keeps_its_position() {
        let mut input = vec![0.0f32; 24_000];
        input.extend(vec![1.0f32; 24_000]);
        let out = run(48_000, &input);
        let at = out.iter().position(|v| *v >= 0.5).expect("no step");
        assert!((at as i64 - 8000).abs() <= 16, "step at {at}");
    }
}
