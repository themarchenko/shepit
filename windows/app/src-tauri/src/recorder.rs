//! Microphone capture through cpal. The stream is built on start and dropped on stop so the
//! microphone is not held while idle.

use crate::controller::Msg;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use shepit_core::audio::CaptureBuffer;
use shepit_core::dictation::{classify_error, CaptureError, Recorder};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

pub struct CpalRecorder {
    tx: SyncSender<Msg>,
    active: Option<Active>,
}

struct Active {
    stream: Stream,
    buffer: Arc<Mutex<CaptureBuffer>>,
}

impl CpalRecorder {
    pub fn new(tx: SyncSender<Msg>) -> Self {
        Self { tx, active: None }
    }
}

fn classify(err: &cpal::Error) -> CaptureError {
    match err.kind() {
        cpal::ErrorKind::PermissionDenied => CaptureError::AccessDenied,
        _ => classify_error(&err.to_string()),
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    buffer: Arc<Mutex<CaptureBuffer>>,
    tx: SyncSender<Msg>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let error_tx = tx.clone();
    device.build_input_stream(
        *config,
        move |data: &[T], _| {
            let floats: Vec<f32> = data.iter().map(|s| s.to_sample::<f32>()).collect();
            // Hold the lock only for the push; sending happens after it is released.
            let levels = match buffer.lock() {
                Ok(mut guard) => guard.push(&floats),
                Err(poisoned) => poisoned.into_inner().push(&floats),
            };
            for level in levels {
                // Never block the audio thread: a dropped level is invisible.
                let _ = tx.try_send(Msg::Level(level));
            }
        },
        // The controller turns this into the Failed state with the microphone message.
        move |err| {
            eprintln!("capture stream error: {err}");
            let _ = error_tx.try_send(Msg::StreamError(classify(&err)));
        },
        None,
    )
}

impl Recorder for CpalRecorder {
    fn start(&mut self) -> Result<(), CaptureError> {
        if self.active.is_some() {
            return Ok(());
        }
        let device = cpal::default_host()
            .default_input_device()
            .ok_or(CaptureError::NoDevice)?;
        let supported = device.default_input_config().map_err(|e| classify(&e))?;
        let config = supported.config();
        let buffer = CaptureBuffer::new(config.sample_rate, config.channels as usize)
            .map_err(CaptureError::Other)?;
        let buffer = Arc::new(Mutex::new(buffer));
        let tx = self.tx.clone();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config, buffer.clone(), tx),
            SampleFormat::I16 => build::<i16>(&device, &config, buffer.clone(), tx),
            SampleFormat::U16 => build::<u16>(&device, &config, buffer.clone(), tx),
            other => {
                return Err(CaptureError::Other(format!(
                    "unsupported sample format {other:?}"
                )))
            }
        }
        .map_err(|e| classify(&e))?;
        stream.play().map_err(|e| classify(&e))?;
        self.active = Some(Active { stream, buffer });
        Ok(())
    }

    fn stop(&mut self) -> Vec<f32> {
        let Some(Active { stream, buffer }) = self.active.take() else {
            return Vec::new();
        };
        // Dropping the stream ends the callbacks before the buffer is read.
        drop(stream);
        let mut guard = match buffer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.finish()
    }
}
