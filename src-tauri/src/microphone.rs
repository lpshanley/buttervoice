//! Captures from the microphone on its own thread, which owns the audio
//! stream from start to stop and collects the audio into memory.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use buttervoice_core::capture::{
    AudioFormat, CaptureReader, Captured, capture_channel, choose_input, input_kind,
};
use buttervoice_core::key_dictation::CaptureCommand;
use buttervoice_core::timeline::Timestamp;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const COLLECT_EVERY: Duration = Duration::from_millis(20);
/// How far collection may fall behind before audio is dropped.
const BUFFERED: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error("there is no input device")]
    NoInputDevice,
    #[error(transparent)]
    Audio(#[from] cpal::Error),
}

struct Capturing {
    stream: cpal::Stream,
    audio: CaptureReader,
    stream_errors: Arc<AtomicU64>,
}

/// `on_stopped` runs on the microphone thread after each stop. It gets
/// `None` when the microphone could not be started.
pub fn spawn(
    on_stopped: impl Fn(Option<Captured>) + Send + 'static,
) -> std::io::Result<Sender<CaptureCommand>> {
    let (commands, received) = mpsc::channel();
    std::thread::Builder::new()
        .name("microphone".into())
        .spawn(move || {
            let mut capturing: Option<Capturing> = None;
            loop {
                let command = match capturing.as_mut() {
                    // Idle: sleep until there is something to do.
                    None => received.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    Some(capturing) => {
                        let command = received.recv_timeout(COLLECT_EVERY);
                        capturing.audio.collect();
                        command
                    }
                };
                match command {
                    Ok(CaptureCommand::Start) => match start() {
                        Ok(started) => capturing = Some(started),
                        Err(error) => tracing::error!(%error, "microphone did not start"),
                    },
                    Ok(CaptureCommand::Stop) => on_stopped(capturing.take().map(stop)),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
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

    let config = device.default_input_config()?.config();
    let format = AudioFormat {
        sample_rate: config.sample_rate,
        channels: config.channels,
    };
    let (mut writer, audio) = capture_channel(input_kind(&chosen), format, BUFFERED);
    let stream_errors = Arc::new(AtomicU64::new(0));
    let stream = device.build_input_stream(
        config,
        move |samples: &[f32], info: &cpal::InputCallbackInfo| {
            let captured_at = info.timestamp().capture.as_nanos() as u64;
            writer.buffer_captured(samples, Timestamp::from_nanos(captured_at));
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
        audio,
        stream_errors,
    })
}

fn stop(capturing: Capturing) -> Captured {
    drop(capturing.stream);
    let stream_errors = capturing.stream_errors.load(Relaxed);
    if stream_errors > 0 {
        tracing::warn!(stream_errors, "microphone reported errors while capturing");
    }
    capturing.audio.finish()
}
