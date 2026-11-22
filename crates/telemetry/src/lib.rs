use std::path::Path;
use std::time::Duration;

use buttervoice_core::recording::StopReason;
use buttervoice_core::timeline::Summary;
use tracing::Subscriber;
use tracing::subscriber::SetGlobalDefaultError;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};
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
        .with_writer(writer);
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

pub fn record_dictation(summary: &Summary, stopped_because: StopReason) {
    tracing::info!(
        dictation_id = summary.dictation_id.0,
        stopped_because = stop_reason_name(stopped_because),
        key_held_ms = summary.key_held.map(millis),
        key_down_to_first_sample_ms = summary.key_down_to_first_sample.map(millis),
        key_up_to_text_landed_ms = summary.key_up_to_text_landed.map(millis),
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

fn millis(duration: Duration) -> f64 {
    duration.as_nanos() as f64 / 1_000_000.0
}
