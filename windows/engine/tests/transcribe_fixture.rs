//! Needs the real model (about 670 MB), so it is ignored by default. CI and the manual run set
//! SHEPIT_MODELS_DIR; the pinned files are downloaded there when missing.

use std::path::{Path, PathBuf};
use std::time::Instant;

use shepit_core::speech::{has_enough_speech, is_junk};
use shepit_engine::download::{ensure, verify_installed, UreqFetch};
use shepit_engine::pins::PARAKEET_V3_INT8;
use shepit_engine::recognizer::Recognizer;

fn read_wav(path: &Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    assert_eq!(
        (spec.channels, spec.sample_rate, spec.bits_per_sample),
        (1, 16_000, 16)
    );
    reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect()
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect())
        .collect()
}

#[test]
#[ignore = "needs the model; set SHEPIT_MODELS_DIR"]
fn manifest_clips_are_transcribed() {
    let models = PathBuf::from(
        std::env::var("SHEPIT_MODELS_DIR")
            .expect("SHEPIT_MODELS_DIR must point at a models folder"),
    );
    let model_dir = models.join(PARAKEET_V3_INT8.dir_name);
    // A restored CI cache is untrusted: re-hash it, drop what does not match, fetch it again.
    verify_installed(&model_dir, &PARAKEET_V3_INT8).unwrap();
    ensure(&model_dir, &PARAKEET_V3_INT8, &UreqFetch, &mut |_, _| {}).unwrap();
    let mut recognizer = Recognizer::load(&model_dir).unwrap();

    let audio = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/audio");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(audio.join("manifest.json")).unwrap())
            .unwrap();
    let clips = manifest.as_array().unwrap();
    assert!(!clips.is_empty());

    for clip in clips {
        let file = clip["file"].as_str().unwrap();
        let samples = read_wav(&audio.join(file));
        assert!(
            has_enough_speech(&samples),
            "{file}: speech gate rejects the clip"
        );

        let start = Instant::now();
        let text = recognizer.transcribe(&samples).unwrap();
        let secs = start.elapsed().as_secs_f64();
        let audio_secs = samples.len() as f64 / 16_000.0;
        println!(
            "{file}: {text:?}\n  {secs:.2} s for {audio_secs:.2} s of audio, real-time factor {:.3}",
            secs / audio_secs
        );

        assert!(!is_junk(&text), "{file}: junk transcript {text:?}");
        let got = words(&text);
        for needed in clip["must_contain"].as_array().unwrap() {
            let needed = needed.as_str().unwrap();
            assert!(
                got.iter().any(|w| w == needed),
                "{file}: {needed:?} missing in {text:?}"
            );
        }
    }
}
