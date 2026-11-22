mod common;

use buttervoice_core::DictationId;
use buttervoice_core::recording::StopReason;
use buttervoice_core::timeline::{Mark, Summary, Timeline, Timestamp};
use buttervoice_telemetry::{file_subscriber, record_dictation};
use serde_json::Value;

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

fn lines_logged_for(summary: &Summary) -> Vec<Value> {
    lines_logged_for_stop(summary, StopReason::KeyReleased)
}

fn lines_logged_for_stop(summary: &Summary, stopped_because: StopReason) -> Vec<Value> {
    let log_dir = tempfile::tempdir().unwrap();
    let (subscriber, guard) = file_subscriber(log_dir.path()).unwrap();
    tracing::subscriber::with_default(subscriber, || record_dictation(summary, stopped_because));
    drop(guard);
    common::read_lines(log_dir.path())
}

#[test]
fn a_dictation_is_logged_as_one_line_with_its_id_and_latencies_in_milliseconds() {
    let mut timeline = Timeline::new(DictationId(7));
    timeline.mark(Mark::KeyDown, at_micros(1_000_000));
    timeline.mark(Mark::FirstSample, at_micros(1_180_250));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));
    timeline.mark(Mark::TextLanded, at_micros(4_612_500));

    let lines = lines_logged_for(&timeline.summary());

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["dictation_id"], 7);
    assert_eq!(lines[0]["key_held_ms"], 3000.0);
    assert_eq!(lines[0]["key_down_to_first_sample_ms"], 180.25);
    assert_eq!(lines[0]["key_up_to_text_landed_ms"], 612.5);
}

#[test]
fn a_latency_that_was_never_measured_is_left_out_of_the_line() {
    let mut timeline = Timeline::new(DictationId(8));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));

    let lines = lines_logged_for(&timeline.summary());

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].get("key_up_to_text_landed_ms"), None);
}

#[test]
fn the_line_says_why_the_dictation_stopped() {
    let summary = Timeline::new(DictationId(9)).summary();

    for (stopped_because, expected) in [
        (StopReason::KeyReleased, "key_released"),
        (StopReason::KeyPressedAgain, "key_pressed_again"),
        (StopReason::Manual, "manual"),
        (StopReason::KeyListenerLost, "key_listener_lost"),
    ] {
        let lines = lines_logged_for_stop(&summary, stopped_because);

        assert_eq!(lines[0]["stopped_because"], expected);
    }
}
