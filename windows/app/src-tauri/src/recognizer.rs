//! The recognizer thread: owns the engine, preloads it at launch and turns jobs into results.
//! Generic over `Engine` so the retry, skip and panic rules run in tests without a model.

use crate::controller::Msg;
use shepit_core::dictation::{Job, ModelStatus};
use shepit_engine::download::{default_models_dir, ensure, is_installed, UreqFetch};
use shepit_engine::pins::PARAKEET_V3_INT8;
use shepit_engine::recognizer::Recognizer;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;

pub trait Engine {
    /// Makes the engine ready: download what is missing (reporting a 0..=1 fraction), then load.
    /// The error text has no "Recognizer failed to load" prefix; the worker adds it.
    fn load(&mut self, progress: &mut dyn FnMut(f32)) -> Result<(), String>;
    /// Errors carry their own "Transcription failed: " prefix.
    fn transcribe(&mut self, samples: &[f32]) -> Result<String, String>;
}

const LOAD_PREFIX: &str = "Recognizer failed to load: ";
const CRASHED: &str = "recognizer crashed";

/// Runs until the job channel closes. `wanted` is the id of the job the controller still wants
/// (0 for none); a job with another id is skipped without a result.
///
/// The engine is loaded once before the first job. A failed load is retried by the next wanted
/// job: a network download can fail for a reason that is gone a minute later.
pub fn run<E: Engine>(
    mut engine: E,
    jobs: Receiver<Job>,
    wanted: Arc<AtomicU64>,
    tx: SyncSender<Msg>,
) {
    let mut ready = load(&mut engine, &tx).is_ok();
    for job in jobs {
        if wanted.load(Ordering::SeqCst) != job.id {
            continue;
        }
        if !ready {
            match load(&mut engine, &tx) {
                Ok(()) => ready = true,
                Err(message) => {
                    let _ = tx.send(Msg::Recognized {
                        id: job.id,
                        result: Err(message),
                    });
                    continue;
                }
            }
        }
        // A panic in the model code must not take the thread down with a job half done.
        let result = catch_unwind(AssertUnwindSafe(|| engine.transcribe(&job.samples)))
            .unwrap_or_else(|_| Err(format!("Transcription failed: {CRASHED}")));
        let _ = tx.send(Msg::Recognized { id: job.id, result });
    }
}

/// One load attempt, reported as model status. The error is the user-facing text.
fn load<E: Engine>(engine: &mut E, tx: &SyncSender<Msg>) -> Result<(), String> {
    let _ = tx.send(Msg::Model(ModelStatus::Loading));
    let mut last_percent: Option<u32> = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        engine.load(&mut |fraction| {
            // The epsilon keeps 0.01_f32 from rounding down to 0 percent.
            let percent = (f64::from(fraction.clamp(0.0, 1.0)) * 100.0 + 1e-4).floor() as u32;
            if last_percent == Some(percent) {
                return;
            }
            last_percent = Some(percent);
            if percent >= 100 {
                // Everything is on disk; what remains is reading it into memory.
                let _ = tx.send(Msg::Model(ModelStatus::Loading));
            } else {
                // Progress is cosmetic: drop it rather than block on a full queue.
                let _ = tx.try_send(Msg::Model(ModelStatus::Downloading(percent as f32 / 100.0)));
            }
        })
    }))
    .unwrap_or_else(|_| Err(CRASHED.to_string()));
    match outcome {
        Ok(()) => {
            let _ = tx.send(Msg::Model(ModelStatus::Ready));
            Ok(())
        }
        Err(message) => {
            let _ = tx.send(Msg::Model(ModelStatus::Failed));
            Err(format!("{LOAD_PREFIX}{message}"))
        }
    }
}

/// Parakeet v3 int8: downloads the pinned files when they are missing, then loads them.
pub struct ParakeetEngine {
    recognizer: Option<Recognizer>,
}

impl ParakeetEngine {
    pub fn new() -> Self {
        Self { recognizer: None }
    }
}

impl Engine for ParakeetEngine {
    fn load(&mut self, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let root = default_models_dir().ok_or("no local data folder for the model")?;
        let dir = root.join(PARAKEET_V3_INT8.dir_name);
        if !is_installed(&dir, &PARAKEET_V3_INT8) {
            ensure(&dir, &PARAKEET_V3_INT8, &UreqFetch, &mut |done, total| {
                progress(done as f32 / total.max(1) as f32)
            })?;
        }
        let recognizer = Recognizer::load(&dir).map_err(|e| {
            // The engine crate words its own error the same way; avoid saying it twice.
            e.strip_prefix(LOAD_PREFIX).unwrap_or(&e).to_string()
        })?;
        self.recognizer = Some(recognizer);
        Ok(())
    }

    fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        match self.recognizer.as_mut() {
            Some(r) => r.transcribe(samples),
            None => Err("Transcription failed: model not loaded".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::mpsc;

    struct Fake {
        loads: VecDeque<Result<(), String>>,
        load_progress: Vec<f32>,
        results: VecDeque<Result<String, String>>,
        panic_on_transcribe: bool,
    }

    impl Fake {
        fn ok() -> Self {
            Self {
                loads: VecDeque::from([Ok(())]),
                load_progress: vec![],
                results: VecDeque::new(),
                panic_on_transcribe: false,
            }
        }
    }

    impl Engine for Fake {
        fn load(&mut self, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
            for p in self.load_progress.clone() {
                progress(p);
            }
            self.loads.pop_front().unwrap_or(Ok(()))
        }
        fn transcribe(&mut self, _samples: &[f32]) -> Result<String, String> {
            if self.panic_on_transcribe {
                panic!("boom");
            }
            self.results.pop_front().unwrap_or(Ok(String::new()))
        }
    }

    fn job(id: u64) -> Job {
        Job {
            id,
            samples: vec![0.0; 4],
        }
    }

    /// Runs the worker over `jobs` (all queued before it starts) and describes every message.
    fn drive(engine: Fake, jobs: &[u64], wanted: u64) -> Vec<String> {
        let (jtx, jrx) = mpsc::channel();
        for id in jobs {
            jtx.send(job(*id)).unwrap();
        }
        drop(jtx);
        let (tx, rx) = mpsc::sync_channel(256);
        run(engine, jrx, Arc::new(AtomicU64::new(wanted)), tx);
        rx.try_iter()
            .map(|m| match m {
                Msg::Model(ModelStatus::Downloading(p)) => format!("downloading {p:.2}"),
                Msg::Model(s) => format!("{s:?}").to_lowercase(),
                Msg::Recognized { id, result } => format!("recognized {id} {result:?}"),
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn preload_reports_ready_then_a_job_gets_its_result() {
        let mut e = Fake::ok();
        e.results.push_back(Ok("hello".into()));
        let out = drive(e, &[7], 7);
        assert_eq!(
            out,
            strings(&["loading", "ready", "recognized 7 Ok(\"hello\")"])
        );
    }

    #[test]
    fn failed_preload_is_failed_and_the_next_job_retries() {
        let mut e = Fake::ok();
        e.loads = VecDeque::from([Err("offline".into()), Ok(())]);
        e.results.push_back(Ok("again".into()));
        let out = drive(e, &[3], 3);
        assert_eq!(
            out,
            strings(&[
                "loading",
                "failed",
                "loading",
                "ready",
                "recognized 3 Ok(\"again\")"
            ])
        );
    }

    #[test]
    fn a_retry_that_fails_again_reports_the_load_error_for_the_job() {
        let mut e = Fake::ok();
        e.loads = VecDeque::from([Err("a".into()), Err("x".into())]);
        let out = drive(e, &[5], 5);
        assert_eq!(
            out,
            strings(&[
                "loading",
                "failed",
                "loading",
                "failed",
                "recognized 5 Err(\"Recognizer failed to load: x\")"
            ])
        );
    }

    #[test]
    fn a_job_that_is_not_wanted_is_skipped_without_a_result() {
        let mut e = Fake::ok();
        e.results.push_back(Ok("second".into()));
        let out = drive(e, &[1, 2], 2);
        assert_eq!(
            out,
            strings(&["loading", "ready", "recognized 2 Ok(\"second\")"])
        );
    }

    #[test]
    fn a_skipped_job_does_not_trigger_a_load_retry() {
        let mut e = Fake::ok();
        e.loads = VecDeque::from([Err("offline".into())]);
        let out = drive(e, &[1], 0);
        assert_eq!(out, strings(&["loading", "failed"]));
    }

    #[test]
    fn a_panic_in_transcribe_becomes_a_failure_result() {
        let mut e = Fake::ok();
        e.panic_on_transcribe = true;
        let out = drive(e, &[9], 9);
        assert_eq!(
            out.last().unwrap(),
            "recognized 9 Err(\"Transcription failed: recognizer crashed\")"
        );
    }

    #[test]
    fn progress_is_sent_only_when_the_whole_percent_changes() {
        let mut e = Fake::ok();
        e.load_progress = vec![0.001, 0.004, 0.01, 0.0149, 0.5, 0.5, 0.504, 1.0];
        let out = drive(e, &[], 0);
        assert_eq!(
            out,
            strings(&[
                "loading",
                "downloading 0.00",
                "downloading 0.01",
                "downloading 0.50",
                // 100% means the download is done and the model is loading.
                "loading",
                "ready"
            ])
        );
    }
}
