use std::path::Path;
use std::time::Duration;

use buttervoice_core::key_dictation::FinishedDictation;
use buttervoice_core::recording::StopReason;
use buttervoice_core::transcription::{Transcription, TranscriptionFailure};
use buttervoice_core::typing::{Typing, TypingFailure};
use tracing::Subscriber;
use tracing::subscriber::SetGlobalDefaultError;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

/// Dropping the guard flushes pending lines and stops the writer, so hold it
/// for as long as the subscriber is in use.
pub fn file_subscriber(
    log_dir: &Path,
) -> Result<(impl Subscriber + Send + Sync, WorkerGuard), InitError> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("buttervoice")
        .filename_suffix("jsonl")
        .build(log_dir)?;
    // Non-blocking so that a slow disk can never delay a dictation.
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_writer(writer)
        // Libraries are chatty below this level, and their detail can name
        // hosts and addresses.
        .with_filter(LevelFilter::INFO);
    Ok((tracing_subscriber::registry().with(file_layer), guard))
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("could not open the log file")]
    LogFile(#[from] InitError),
    #[error("telemetry is already installed in this process")]
    AlreadyInstalled(#[from] SetGlobalDefaultError),
}

pub fn install(log_dir: &Path) -> Result<WorkerGuard, InstallError> {
    let (subscriber, guard) = file_subscriber(log_dir)?;
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(guard)
}

pub fn record_dictation(dictation: &FinishedDictation) {
    let summary = &dictation.summary;
    tracing::info!(
        dictation_id = summary.dictation_id.0,
        stopped_because = stop_reason_name(dictation.stopped_because),
        key_held_ms = summary.key_held.map(millis),
        captured_audio_ms = dictation.audio.map(|audio| millis(audio.duration)),
        dropped_samples = dictation.audio.map(|audio| audio.dropped_samples),
        audio_peak_dbfs = dictation.audio.and_then(|audio| dbfs(audio.peak)),
        audio_rms_dbfs = dictation.audio.and_then(|audio| dbfs(audio.rms)),
        key_down_to_first_sample_ms = summary.key_down_to_first_sample.map(millis),
        key_down_to_first_sound_ms = summary.key_down_to_first_sound.map(millis),
        key_up_to_capture_stopped_ms = summary.key_up_to_capture_stopped.map(millis),
        transcription = transcription_outcome(&dictation.transcription),
        transcription_failure = transcription_failure(&dictation.transcription),
        transcription_ms = summary.transcription.map(millis),
        transcript_chars = transcript_chars(&dictation.transcription),
        typing = typing_outcome(dictation.typing),
        typing_failure = typing_failure(dictation.typing),
        key_up_to_text_typed_ms = summary.key_up_to_text_typed.map(millis),
        "dictation"
    );
}

fn stop_reason_name(reason: StopReason) -> &'static str {
    match reason {
        StopReason::KeyReleased => "key_released",
        StopReason::KeyPressedAgain => "key_pressed_again",
        StopReason::Manual => "manual",
        StopReason::KeyListenerLost => "key_listener_lost",
    }
}

fn transcription_outcome(transcription: &Transcription) -> &'static str {
    match transcription {
        Transcription::NotAttempted => "not_attempted",
        Transcription::Succeeded(_) => "succeeded",
        Transcription::Failed(_) => "failed",
    }
}

fn transcription_failure(transcription: &Transcription) -> Option<&'static str> {
    let Transcription::Failed(failure) = transcription else {
        return None;
    };
    Some(match failure {
        TranscriptionFailure::NotConfigured => "not_configured",
        TranscriptionFailure::Unauthorized => "unauthorized",
        TranscriptionFailure::Unavailable => "unavailable",
        TranscriptionFailure::Rejected => "rejected",
        TranscriptionFailure::Unreachable => "unreachable",
    })
}

fn typing_outcome(typing: Typing) -> &'static str {
    match typing {
        Typing::NotAttempted => "not_attempted",
        Typing::Typed => "typed",
        Typing::Failed(_) => "failed",
    }
}

fn typing_failure(typing: Typing) -> Option<&'static str> {
    match typing {
        Typing::Failed(TypingFailure::NotPermitted) => Some("not_permitted"),
        _ => None,
    }
}

/// The length is logged; the words never are.
fn transcript_chars(transcription: &Transcription) -> Option<u64> {
    match transcription {
        Transcription::Succeeded(transcript) => Some(transcript.text.chars().count() as u64),
        _ => None,
    }
}

/// Decibels relative to full scale, to one decimal place. Pure silence has
/// no level on this scale.
fn dbfs(level: f32) -> Option<f64> {
    (level > 0.0).then(|| (20.0 * f64::from(level).log10() * 10.0).round() / 10.0)
}

fn millis(duration: Duration) -> f64 {
    duration.as_nanos() as f64 / 1_000_000.0
}
