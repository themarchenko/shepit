//! The "Test microphone" button: open the default input for one second, throw the samples away and
//! say how it went. Nothing is stored or logged, and it runs only when the user asks, so the
//! microphone indicator never lights up unasked.

use crate::controller::Msg;
use crate::recorder::CpalRecorder;
use shepit_core::dictation::{CaptureError, Recorder};
use shepit_core::mic::{classify, MicOutcome};
use std::sync::mpsc;
use std::time::Duration;

const LISTEN: Duration = Duration::from_millis(1000);

/// Runs the check on the calling thread (the stream may not leave it).
pub fn run() -> MicOutcome {
    // The recorder reports levels and stream errors here; the check keeps only the errors.
    let (tx, rx) = mpsc::sync_channel::<Msg>(1);
    let mut recorder = CpalRecorder::new(tx);
    let mut result = capture(&mut recorder, LISTEN);
    if result.is_ok() {
        if let Some(err) = rx.try_iter().find_map(|m| match m {
            Msg::StreamError(e) => Some(e),
            _ => None,
        }) {
            result = Err(err);
        }
    }
    classify(result)
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
}
