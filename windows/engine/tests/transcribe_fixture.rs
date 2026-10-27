//! Needs the real model (about 670 MB), so it is ignored by default. CI and the manual run set
//! SHEPIT_MODELS_DIR; the pinned files are downloaded there when missing.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Instant;

use shepit_core::live::{Config, LivePreview, LiveSource};
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

/// Downloads or verifies the pinned files under SHEPIT_MODELS_DIR, then loads the model.
fn load_recognizer() -> Recognizer {
    let models = PathBuf::from(
        std::env::var("SHEPIT_MODELS_DIR")
            .expect("SHEPIT_MODELS_DIR must point at a models folder"),
    );
    let model_dir = models.join(PARAKEET_V3_INT8.dir_name);
    // A restored CI cache is untrusted: re-hash it, drop what does not match, fetch it again.
    verify_installed(&model_dir, &PARAKEET_V3_INT8).unwrap();
    ensure(&model_dir, &PARAKEET_V3_INT8, &UreqFetch, &mut |_, _| {}).unwrap();
    Recognizer::load(&model_dir).unwrap()
}

#[test]
#[ignore = "needs the model; set SHEPIT_MODELS_DIR"]
fn manifest_clips_are_transcribed() {
    let mut recognizer = load_recognizer();

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

/// A recording whose length follows a virtual clock, so a pass that took 0.4 s of real time
/// moves the clock by 0.4 s and the buffer grows by as much audio.
struct VirtualRecording {
    audio: Vec<f32>,
    now: Cell<f64>,
}

impl LiveSource for VirtualRecording {
    fn captured(&self) -> usize {
        ((self.now.get() * 16_000.0) as usize).min(self.audio.len())
    }
    fn samples_from(&self, start: usize) -> Vec<f32> {
        self.audio[start.min(self.captured())..self.captured()].to_vec()
    }
}

#[test]
#[ignore = "needs the model; set SHEPIT_MODELS_DIR"]
fn live_preview_follows_the_fixture_clip() {
    let mut recognizer = load_recognizer();
    let audio_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/audio");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(audio_dir.join("manifest.json")).unwrap())
            .unwrap();
    let clip = &manifest.as_array().unwrap()[0];
    let one = read_wav(&audio_dir.join(clip["file"].as_str().unwrap()));
    // The clip twice (about 15 s), so the freeze runs.
    let mut audio = one.clone();
    audio.extend_from_slice(&one);
    let audio_secs = audio.len() as f64 / 16_000.0;
    assert!(
        audio_secs > 6.5 + 0.5,
        "the clip must be long enough to freeze"
    );

    // A preview pass must not change what the final pass says.
    let final_before = recognizer.transcribe(&audio).unwrap();

    let config = Config {
        slow_pass_secs: f64::INFINITY,
        ..Config::default()
    };
    let max_request = config.freeze_after + 3 * 16_000;
    let mut preview = LivePreview::new(config);
    preview.start(0.0);
    let recording = VirtualRecording {
        audio,
        now: Cell::new(0.0),
    };

    let mut v = 0.0;
    let mut updates = 0;
    let mut first_text_at: Option<f64> = None;
    let mut passes: Vec<(f64, usize)> = Vec::new();
    // A request issued at the very moment the previous one completed follows a freeze, so the
    // earlier result was frozen text.
    let mut last: Option<(f64, String)> = None;
    let mut frozen_texts: Vec<String> = Vec::new();
    while v < audio_secs {
        recording.now.set(v);
        match preview.next(v, &recording) {
            Some(req) => {
                assert!(
                    req.samples.len() <= max_request,
                    "a request of {} samples at {v:.2} s",
                    req.samples.len()
                );
                if let Some((done_at, text)) = last.take() {
                    if done_at == v {
                        frozen_texts.push(text);
                    }
                }
                let started = Instant::now();
                let text = recognizer.transcribe(&req.samples).unwrap();
                let secs = started.elapsed().as_secs_f64();
                passes.push((secs, req.samples.len()));
                println!(
                    "pass at {v:.2} s: {} samples, {secs:.2} s",
                    req.samples.len()
                );
                v += secs;
                recording.now.set(v);
                let before = preview.text().full();
                preview.on_result(req.seq, Ok(text.clone()), secs, v);
                if preview.text().full() != before {
                    updates += 1;
                    first_text_at.get_or_insert(v);
                }
                last = Some((v, text));
            }
            None => v += 0.05,
        }
    }
    preview.stop();

    let longest = passes.iter().map(|p| p.0).fold(0.0, f64::max);
    println!(
        "{} passes, longest {longest:.2} s; the default 1.0 s guard would {}",
        passes.len(),
        if longest > 1.0 { "trip" } else { "not trip" }
    );

    assert!(updates >= 3, "only {updates} text updates");
    let first = first_text_at.expect("no text appeared");
    assert!(first <= 3.0, "first text at {first:.2} s of virtual time");
    assert!(!frozen_texts.is_empty(), "the freeze never ran");
    for t in &frozen_texts {
        assert!(!t.trim().is_empty() && !is_junk(t), "frozen text {t:?}");
    }
    let shown = words(&preview.text().full());
    for needed in clip["must_contain"].as_array().unwrap() {
        let needed = needed.as_str().unwrap();
        assert!(
            shown.iter().any(|w| w == needed),
            "{needed:?} missing in {:?}",
            preview.text().full()
        );
    }

    let final_after = recognizer.transcribe(&recording.audio).unwrap();
    assert_eq!(
        final_before, final_after,
        "previews changed the final result"
    );
}
