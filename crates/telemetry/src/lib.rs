use std::path::Path;
use std::time::Duration;

use buttervoice_core::timeline::Summary;
use tracing::Subscriber;
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

pub fn record_dictation(summary: &Summary) {
    tracing::info!(
        dictation_id = summary.dictation_id.0,
        key_down_to_first_sample_ms = summary.key_down_to_first_sample.map(millis),
        key_up_to_text_landed_ms = summary.key_up_to_text_landed.map(millis),
        "dictation"
    );
}

fn millis(duration: Duration) -> f64 {
    duration.as_nanos() as f64 / 1_000_000.0
}
