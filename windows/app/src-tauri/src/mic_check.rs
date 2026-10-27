//! The "Test microphone" button: open the default input for one second, throw the samples away and
//! say how it went. Nothing is stored or logged, and it runs only when the user asks, so the
//! microphone indicator never lights up unasked.

use crate::controller::Msg;
use crate::recorder::{CpalRecorder, ErrorSlot};
use shepit_core::dictation::{CaptureError, Recorder};
use shepit_core::mic::{classify, MicOutcome};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

const LISTEN: Duration = Duration::from_millis(1000);

/// True while a check is open, so a double click cannot open a second capture.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Holds the right to run; gives it back on drop, also after a panic.
struct Running<'a>(&'a AtomicBool);

impl<'a> Running<'a> {
    fn acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(flag))
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// The detail of the "failed" result when another check is already running (or a dictation is).
pub const BUSY: &str = "busy";

/// Runs the check on the calling thread (the stream may not leave it).
pub fn run() -> MicOutcome {
    let Some(_running) = Running::acquire(&RUNNING) else {
        return MicOutcome::Failed(BUSY.to_string());
    };
    // Levels go nowhere useful here, so the channel is tiny and may drop them. A stream error is also
    // written to the slot, which cannot be full, so a broken stream is never reported as fine.
    let (tx, _rx) = mpsc::sync_channel::<Msg>(1);
    let slot = ErrorSlot::default();
    let mut recorder = CpalRecorder::new(tx).with_error_slot(slot.clone());
    let heard = capture(&mut recorder, LISTEN);
    classify(with_stream_error(heard, &slot))
}

/// A stream error that came in while listening beats what was heard.
fn with_stream_error(
    heard: Result<Vec<f32>, CaptureError>,
    slot: &ErrorSlot,
) -> Result<Vec<f32>, CaptureError> {
    let error = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
    match (heard, error) {
        (Ok(_), Some(err)) => Err(err),
        (heard, _) => heard,
    }
}

/// Starts `recorder`, waits `listen` and returns what it heard.
fn capture<R: Recorder>(recorder: &mut R, listen: Duration) -> Result<Vec<f32>, CaptureError> {
    recorder.start()?;
    std::thread::sleep(listen);
    Ok(recorder.stop())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(Result<(), CaptureError>, Vec<f32>);

    impl Recorder for Fake {
        fn start(&mut self) -> Result<(), CaptureError> {
            self.0.clone()
        }
        fn stop(&mut self) -> Vec<f32> {
            self.1.clone()
        }
    }

    #[test]
    fn a_denied_start_is_denied() {
        let mut r = Fake(Err(CaptureError::AccessDenied), vec![]);
        let got = capture(&mut r, Duration::ZERO);
        assert_eq!(classify(got), MicOutcome::Denied);
    }

    #[test]
    fn heard_samples_are_ok_and_zeros_are_silent() {
        let mut r = Fake(Ok(()), vec![0.0, 0.2, -0.1]);
        assert_eq!(classify(capture(&mut r, Duration::ZERO)), MicOutcome::Ok);
        let mut r = Fake(Ok(()), vec![0.0; 100]);
        assert_eq!(
            classify(capture(&mut r, Duration::ZERO)),
            MicOutcome::Silent
        );
    }

    #[test]
    fn a_stream_error_beats_samples_that_look_fine() {
        let slot = ErrorSlot::default();
        crate::recorder::note_error(&slot, CaptureError::NoDevice);
        let got = with_stream_error(Ok(vec![0.0, 0.3]), &slot);
        assert_eq!(classify(got), MicOutcome::NoDevice);
        // Taken once: the next check starts clean.
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn without_a_stream_error_the_result_stands() {
        let slot = ErrorSlot::default();
        assert!(with_stream_error(Ok(vec![0.3]), &slot).is_ok());
        let err = with_stream_error(Err(CaptureError::AccessDenied), &slot);
        assert_eq!(classify(err), MicOutcome::Denied);
    }

    #[test]
    fn a_second_check_is_refused_until_the_first_ends() {
        let flag = AtomicBool::new(false);
        let first = Running::acquire(&flag);
        assert!(first.is_some());
        assert!(Running::acquire(&flag).is_none());
        drop(first);
        assert!(Running::acquire(&flag).is_some());
    }
}
