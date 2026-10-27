//! The recognizer thread: owns the engine, preloads it at launch and turns jobs into results.
//! Generic over `Engine` so the retry, skip and panic rules run in tests without a model.

use crate::controller::Msg;
use shepit_core::dictation::{Job, ModelStatus, PreviewJob};
use shepit_engine::download::{
    default_models_dir, ensure, is_installed, verify_installed, UreqFetch,
};
use shepit_engine::pins::PARAKEET_V3_INT8;
use shepit_engine::recognizer::Recognizer;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;
use std::time::Instant;

pub trait Engine {
    /// Makes the engine ready: download what is missing (reporting a 0..=1 fraction), then load.
    /// The error text has no "Recognizer failed to load" prefix; the worker adds it.
    fn load(&mut self, progress: &mut dyn FnMut(f32)) -> Result<(), String>;
    /// Errors carry their own "Transcription failed: " prefix.
    fn transcribe(&mut self, samples: &[f32]) -> Result<String, String>;
}

const LOAD_PREFIX: &str = "Recognizer failed to load: ";
const CRASHED: &str = "recognizer crashed";

/// One unit of work for the recognizer thread. Live preview passes share the thread (and the
/// one loaded model) with the finals.
pub enum Work {
    Final(Job),
    Preview(PreviewJob),
}

/// Runs until the work channel closes. `wanted` is the id of the job the controller still wants
/// (0 for none); a job with another id is skipped without a result. `preview_wanted` is the same
/// for preview passes: the controller sets it to 0 on release, before the final job is sent, so
/// a queued preview never delays the final.
///
/// The engine is loaded once before the first job. A failed load is retried by the next wanted
/// job: a network download can fail for a reason that is gone a minute later. A preview never
/// triggers a load.
pub fn run<E: Engine>(
    mut engine: E,
    work: Receiver<Work>,
    wanted: Arc<AtomicU64>,
    preview_wanted: Arc<AtomicU64>,
    tx: SyncSender<Msg>,
) {
    let mut ready = load(&mut engine, &tx).is_ok();
    for item in work {
        let job = match item {
            Work::Final(job) => job,
            Work::Preview(pass) => {
                if preview_wanted.load(Ordering::SeqCst) != pass.id {
                    continue;
                }
                let started = Instant::now();
                let result = if ready {
                    catch_unwind(AssertUnwindSafe(|| engine.transcribe(&pass.samples)))
                        .unwrap_or_else(|_| Err(format!("Transcription failed: {CRASHED}")))
                } else {
                    Err("Transcription failed: model not ready".to_string())
                };
                let secs = started.elapsed().as_secs_f64();
                // try_send: a stalled controller must not block this thread. A lost result leaves
                // the pass in flight in the scheduler, so the preview stops for this dictation and
                // keeps its text, like the slow-pass stop.
                let _ = tx.try_send(Msg::Previewed {
                    id: pass.id,
                    seq: pass.seq,
                    result,
                    secs,
                });
                continue;
            }
        };
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
        // A panic in the model code must not take the thread down with a job half done. The time
        // covers `transcribe` only (DictationSession.swift:171-180) and goes first, so the
        // controller has it when the result arrives.
        let started = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| engine.transcribe(&job.samples)))
            .unwrap_or_else(|_| Err(format!("Transcription failed: {CRASHED}")));
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let _ = tx.send(Msg::SttMs { id: job.id, ms });
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
        let fetch = |progress: &mut dyn FnMut(f32)| {
            ensure(&dir, &PARAKEET_V3_INT8, &UreqFetch, &mut |done, total| {
                progress(done as f32 / total.max(1) as f32)
            })
        };
        if !is_installed(&dir, &PARAKEET_V3_INT8) {
            fetch(progress)?;
        }
        let strip = |e: String| e.strip_prefix(LOAD_PREFIX).unwrap_or(&e).to_string();
        let recognizer = match Recognizer::load(&dir) {
            Ok(r) => r,
            Err(first) => {
                // Installed files are trusted on size at launch. A failed load may mean a
                // corrupted one: re-hash, drop bad files, download them again, try once more.
                eprintln!("model load failed ({first}), verifying the installed files");
                verify_installed(&dir, &PARAKEET_V3_INT8)?;
                fetch(progress)?;
                // The engine crate words its own error the same way; avoid saying it twice.
                Recognizer::load(&dir).map_err(strip)?
            }
        };
        let mut recognizer = recognizer;
        // Ready is reported after this, so the first dictation does not pay the cold pass. A
        // failure is not a failed load; the text is not logged because it could hold a transcript.
        if recognizer.warm_up().is_err() {
            eprintln!("model warm-up failed, continuing");
        }
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
    use shepit_core::dictation::PreviewJob;
    use std::collections::VecDeque;
    use std::sync::mpsc;

    struct Fake {
        loads: VecDeque<Result<(), String>>,
        load_progress: Vec<f32>,
        results: VecDeque<Result<String, String>>,
        panic_on_transcribe: bool,
        /// How long `transcribe` takes.
        delay: std::time::Duration,
    }

    impl Fake {
        fn ok() -> Self {
            Self {
                loads: VecDeque::from([Ok(())]),
                load_progress: vec![],
                results: VecDeque::new(),
                panic_on_transcribe: false,
                delay: std::time::Duration::ZERO,
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
            std::thread::sleep(self.delay);
            if self.panic_on_transcribe {
                panic!("boom");
            }
            self.results.pop_front().unwrap_or(Ok(String::new()))
        }
    }

    fn job(id: u64) -> Work {
        Work::Final(Job {
            id,
            samples: vec![0.0; 4],
        })
    }

    fn preview(id: u64, seq: u64) -> Work {
        Work::Preview(PreviewJob {
            id,
            seq,
            samples: vec![0.0; 4],
        })
    }

    /// Runs the worker over `jobs` (all queued before it starts) and describes every message.
    fn drive(engine: Fake, jobs: &[u64], wanted: u64) -> Vec<String> {
        drive_work(engine, jobs.iter().map(|id| job(*id)).collect(), wanted, 0)
    }

    /// Like `drive` for any mix of finals and previews; `preview_wanted` is the dictation whose
    /// previews are still wanted (0 for none).
    fn drive_work(engine: Fake, work: Vec<Work>, wanted: u64, preview_wanted: u64) -> Vec<String> {
        let (jtx, jrx) = mpsc::channel();
        for w in work {
            jtx.send(w).unwrap();
        }
        drop(jtx);
        let (tx, rx) = mpsc::sync_channel(256);
        run(
            engine,
            jrx,
            Arc::new(AtomicU64::new(wanted)),
            Arc::new(AtomicU64::new(preview_wanted)),
            tx,
        );
        rx.try_iter()
            .filter_map(|m| match m {
                Msg::Model(ModelStatus::Downloading(p)) => Some(format!("downloading {p:.2}")),
                Msg::Model(s) => Some(format!("{s:?}").to_lowercase()),
                Msg::Recognized { id, result } => Some(format!("recognized {id} {result:?}")),
                Msg::Previewed {
                    id, seq, result, ..
                } => Some(format!("previewed {id} {seq} {result:?}")),
                // Timing has its own test below.
                Msg::SttMs { .. } => None,
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_stt_time_comes_from_the_engine_and_arrives_before_the_result() {
        let mut e = Fake::ok();
        e.delay = std::time::Duration::from_millis(40);
        e.results.push_back(Ok("hello".into()));
        let (jtx, jrx) = mpsc::channel();
        jtx.send(job(7)).unwrap();
        drop(jtx);
        let (tx, rx) = mpsc::sync_channel(256);
        run(
            e,
            jrx,
            Arc::new(AtomicU64::new(7)),
            Arc::new(AtomicU64::new(0)),
            tx,
        );
        let sent: Vec<Msg> = rx.try_iter().collect();
        let timed = sent
            .iter()
            .position(|m| matches!(m, Msg::SttMs { id: 7, .. }))
            .expect("an SttMs message");
        let result = sent
            .iter()
            .position(|m| matches!(m, Msg::Recognized { id: 7, .. }))
            .expect("a Recognized message");
        assert!(timed < result, "the time must reach the controller first");
        let Msg::SttMs { ms, .. } = &sent[timed] else {
            unreachable!()
        };
        assert!((40..5_000).contains(ms), "{ms} ms");
    }

    #[test]
    fn a_skipped_job_sends_no_stt_time() {
        let mut e = Fake::ok();
        e.results.push_back(Ok("hello".into()));
        let (jtx, jrx) = mpsc::channel();
        jtx.send(job(7)).unwrap();
        drop(jtx);
        let (tx, rx) = mpsc::sync_channel(256);
        run(
            e,
            jrx,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            tx,
        );
        assert!(!rx.try_iter().any(|m| matches!(m, Msg::SttMs { .. })));
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

    // Live preview passes share the thread with the finals.

    #[test]
    fn a_wanted_preview_gets_its_result_with_its_seq() {
        let mut e = Fake::ok();
        e.results.push_back(Ok("hello there".into()));
        let out = drive_work(e, vec![preview(7, 3)], 0, 7);
        assert_eq!(
            out,
            strings(&["loading", "ready", "previewed 7 3 Ok(\"hello there\")"])
        );
    }

    #[test]
    fn a_preview_is_timed_and_sends_no_stt_time_or_final_result() {
        let mut e = Fake::ok();
        e.delay = std::time::Duration::from_millis(40);
        e.results.push_back(Ok("hello".into()));
        let (jtx, jrx) = mpsc::channel();
        jtx.send(preview(7, 1)).unwrap();
        drop(jtx);
        let (tx, rx) = mpsc::sync_channel(256);
        run(
            e,
            jrx,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(7)),
            tx,
        );
        let sent: Vec<Msg> = rx.try_iter().collect();
        assert!(!sent
            .iter()
            .any(|m| matches!(m, Msg::SttMs { .. } | Msg::Recognized { .. })));
        let secs = sent
            .iter()
            .find_map(|m| match m {
                Msg::Previewed { secs, .. } => Some(*secs),
                _ => None,
            })
            .expect("a Previewed message");
        assert!((0.04..5.0).contains(&secs), "{secs} s");
    }

    #[test]
    fn a_preview_that_is_not_wanted_is_skipped_silently() {
        let mut e = Fake::ok();
        e.results.push_back(Ok("hello".into()));
        let out = drive_work(e, vec![preview(7, 1)], 7, 8);
        assert_eq!(out, strings(&["loading", "ready"]));
        let out = drive_work(Fake::ok(), vec![preview(7, 1)], 7, 0);
        assert_eq!(out, strings(&["loading", "ready"]));
    }

    #[test]
    fn a_preview_before_the_model_is_ready_does_not_load_and_gets_an_error() {
        let mut e = Fake::ok();
        e.loads = VecDeque::from([Err("offline".into()), Ok(())]);
        let out = drive_work(e, vec![preview(7, 2)], 0, 7);
        assert_eq!(out.len(), 3, "no second load attempt: {out:?}");
        assert_eq!(&out[..2], strings(&["loading", "failed"]));
        assert!(out[2].starts_with("previewed 7 2 Err("), "{}", out[2]);
    }

    #[test]
    fn a_final_queued_behind_an_unwanted_preview_runs_next() {
        let mut e = Fake::ok();
        e.results.push_back(Ok("final".into()));
        let out = drive_work(e, vec![preview(5, 1), job(6)], 6, 0);
        assert_eq!(
            out,
            strings(&["loading", "ready", "recognized 6 Ok(\"final\")"])
        );
    }

    #[test]
    fn a_preview_then_the_final_both_run_in_order() {
        let mut e = Fake::ok();
        e.results.push_back(Ok("preview words".into()));
        e.results.push_back(Ok("final words".into()));
        let out = drive_work(e, vec![preview(6, 1), job(6)], 6, 6);
        assert_eq!(
            out,
            strings(&[
                "loading",
                "ready",
                "previewed 6 1 Ok(\"preview words\")",
                "recognized 6 Ok(\"final words\")"
            ])
        );
    }

    #[test]
    fn a_panic_in_a_preview_becomes_an_error_result() {
        let mut e = Fake::ok();
        e.panic_on_transcribe = true;
        let out = drive_work(e, vec![preview(9, 4)], 0, 9);
        assert!(
            out.last().unwrap().starts_with("previewed 9 4 Err("),
            "{out:?}"
        );
    }

    #[test]
    fn the_thread_survives_a_preview_panic() {
        let mut e = Fake::ok();
        e.panic_on_transcribe = true;
        let out = drive_work(e, vec![preview(9, 1), preview(9, 2)], 0, 9);
        let previews = out.iter().filter(|m| m.starts_with("previewed")).count();
        assert_eq!(previews, 2, "{out:?}");
    }
}
