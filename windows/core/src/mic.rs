//! Maps the result of the one-second microphone check to what Settings shows. The device work
//! lives in the app; this stays pure so it runs in tests.

use crate::audio::is_digital_silence;
use crate::dictation::CaptureError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicOutcome {
    Ok,
    /// The device opened but delivered only digital silence.
    Silent,
    Denied,
    NoDevice,
    Failed(String),
}

impl MicOutcome {
    /// The id the UI switches on: "ok", "silent", "denied", "noDevice", "failed".
    pub fn id(&self) -> &'static str {
        match self {
            MicOutcome::Ok => "ok",
            MicOutcome::Silent => "silent",
            MicOutcome::Denied => "denied",
            MicOutcome::NoDevice => "noDevice",
            MicOutcome::Failed(_) => "failed",
        }
    }
}

/// `Ok` carries the samples captured in the check, `Err` the capture error. AccessDenied gives
/// Denied, NoDevice gives NoDevice, any other error gives Failed with its text, no samples or
/// only samples within 1e-6 of zero give Silent, anything else Ok.
pub fn classify(result: Result<Vec<f32>, CaptureError>) -> MicOutcome {
    match result {
        Err(CaptureError::AccessDenied) => MicOutcome::Denied,
        Err(CaptureError::NoDevice) => MicOutcome::NoDevice,
        Err(CaptureError::Silent) => MicOutcome::Silent,
        Err(other) => MicOutcome::Failed(other.to_string()),
        Ok(samples) if is_digital_silence(&samples) => MicOutcome::Silent,
        Ok(_) => MicOutcome::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_denied_is_denied() {
        assert_eq!(
            classify(Err(CaptureError::AccessDenied)),
            MicOutcome::Denied
        );
    }

    #[test]
    fn no_device_is_no_device() {
        assert_eq!(classify(Err(CaptureError::NoDevice)), MicOutcome::NoDevice);
    }

    #[test]
    fn any_other_error_is_failed_with_its_text() {
        assert_eq!(
            classify(Err(CaptureError::Other("stream broke".into()))),
            MicOutcome::Failed("stream broke".into())
        );
    }

    #[test]
    fn a_silent_error_from_the_recorder_is_not_a_device_problem() {
        // The recorder never returns Silent itself; if it does, the check says so.
        assert_eq!(classify(Err(CaptureError::Silent)), MicOutcome::Silent);
    }

    #[test]
    fn no_samples_is_silent() {
        assert_eq!(classify(Ok(vec![])), MicOutcome::Silent);
    }

    #[test]
    fn all_zero_samples_are_silent() {
        assert_eq!(classify(Ok(vec![0.0; 16_000])), MicOutcome::Silent);
    }

    #[test]
    fn samples_at_the_1e_minus_6_edge_are_silent() {
        assert_eq!(classify(Ok(vec![1e-6, -1e-6, 0.0])), MicOutcome::Silent);
    }

    #[test]
    fn a_sample_above_the_edge_is_ok() {
        assert_eq!(classify(Ok(vec![0.0, 2e-6, 0.0])), MicOutcome::Ok);
    }

    #[test]
    fn normal_audio_is_ok() {
        assert_eq!(classify(Ok(vec![0.1; 16_000])), MicOutcome::Ok);
    }

    #[test]
    fn ids_are_the_names_the_ui_switches_on() {
        assert_eq!(MicOutcome::Ok.id(), "ok");
        assert_eq!(MicOutcome::Silent.id(), "silent");
        assert_eq!(MicOutcome::Denied.id(), "denied");
        assert_eq!(MicOutcome::NoDevice.id(), "noDevice");
        assert_eq!(MicOutcome::Failed("x".into()).id(), "failed");
    }
}
