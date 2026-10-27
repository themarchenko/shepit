//! Parakeet v3 (int8, CPU) through transcribe-rs.

use std::path::Path;

use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

pub struct Recognizer {
    model: ParakeetModel,
}

impl Recognizer {
    /// `dir` holds the four files of `pins::PARAKEET_V3_INT8`. The int8 quantization is explicit
    /// because transcribe-rs silently falls back to the fp32 file name when an int8 one is missing.
    pub fn load(dir: &Path) -> Result<Self, String> {
        ParakeetModel::load(dir, &Quantization::Int8)
            .map(|model| Self { model })
            .map_err(|e| format!("Recognizer failed to load: {e}"))
    }

    /// `samples`: 16 kHz mono f32 in [-1, 1]. Language is auto-detected by the model, so no hint.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        self.model
            .transcribe(samples, &TranscribeOptions::default())
            .map(|r| r.text.trim().to_string())
            .map_err(|e| format!("Transcription failed: {e}"))
    }

    /// Runs one second of silence through the model and drops the text. The first call after a
    /// load is slower than the rest (0.90 s against about 0.39 s on the Mac), and a slow first
    /// preview pass would trip the slow-pass stop before the dictation had any preview.
    pub fn warm_up(&mut self) -> Result<(), String> {
        self.transcribe(&[0.0; 16_000]).map(|_| ())
    }
}
