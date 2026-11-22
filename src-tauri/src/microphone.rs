//! Captures from the microphone on its own thread, which owns the audio
//! stream from start to stop.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{self, Sender};

use buttervoice_core::capture::{CaptureMarks, CaptureReport, choose_input};
use buttervoice_core::key_dictation::CaptureCommand;
use buttervoice_core::timeline::Timestamp;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error("there is no input device")]
    NoInputDevice,
    #[error(transparent)]
    Audio(#[from] cpal::Error),
}

struct Capturing {
    stream: cpal::Stream,
    marks: Arc<CaptureMarks>,
    stream_errors: Arc<AtomicU64>,
}

/// `on_stopped` runs on the microphone thread after each stop, including a
/// stop that follows a start that failed.
pub fn spawn(
    on_stopped: impl Fn(CaptureReport) + Send + 'static,
) -> std::io::Result<Sender<CaptureCommand>> {
    let (commands, received) = mpsc::channel();
    std::thread::Builder::new()
        .name("microphone".into())
        .spawn(move || {
            let mut capturing = None;
            for command in received {
                match command {
                    CaptureCommand::Start => match start() {
                        Ok(started) => capturing = Some(started),
                        Err(error) => tracing::error!(%error, "microphone did not start"),
                    },
                    CaptureCommand::Stop => on_stopped(stop(capturing.take())),
                }
            }
        })?;
    Ok(commands)
}

fn start() -> Result<Capturing, StartError> {
    let host = cpal::default_host();
    let inputs: Vec<(String, cpal::Device)> = host
        .input_devices()?
        .filter_map(|device| Some((device.id().ok()?.id().to_string(), device)))
        .collect();
    let system_default = host
        .default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.id().to_string());
    let available: Vec<&str> = inputs.iter().map(|(id, _)| id.as_str()).collect();
    let chosen = choose_input(&available, system_default.as_deref())
        .ok_or(StartError::NoInputDevice)?
        .to_string();
    let (_, device) = inputs
        .into_iter()
        .find(|(id, _)| *id == chosen)
        .ok_or(StartError::NoInputDevice)?;

    let marks = Arc::new(CaptureMarks::new());
    let stream_errors = Arc::new(AtomicU64::new(0));
    let stream = device.build_input_stream(
        device.default_input_config()?.config(),
        {
            let marks = marks.clone();
            move |samples: &[f32], info: &cpal::InputCallbackInfo| {
                let captured_at = info.timestamp().capture.as_nanos() as u64;
                marks.buffer_captured(samples, Timestamp::from_nanos(captured_at));
            }
        },
        {
            // This may run on the audio thread, so it only counts.
            let stream_errors = stream_errors.clone();
            move |_error| {
                stream_errors.fetch_add(1, Relaxed);
            }
        },
        None,
    )?;
    stream.play()?;
    Ok(Capturing {
        stream,
        marks,
        stream_errors,
    })
}

fn stop(capturing: Option<Capturing>) -> CaptureReport {
    let Some(capturing) = capturing else {
        return CaptureReport::default();
    };
    drop(capturing.stream);
    let stream_errors = capturing.stream_errors.load(Relaxed);
    if stream_errors > 0 {
        tracing::warn!(stream_errors, "microphone reported errors while capturing");
    }
    capturing.marks.report()
}
