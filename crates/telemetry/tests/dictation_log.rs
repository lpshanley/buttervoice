mod common;

use std::time::Duration;

use buttervoice_core::DictationId;
use buttervoice_core::capture::AudioStats;
use buttervoice_core::key_dictation::FinishedDictation;
use buttervoice_core::recording::StopReason;
use buttervoice_core::timeline::{Mark, Summary, Timeline, Timestamp};
use buttervoice_core::transcription::{Transcript, Transcription, TranscriptionFailure};
use buttervoice_telemetry::{file_subscriber, record_dictation};
use serde_json::Value;

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

fn lines_logged_for(summary: &Summary) -> Vec<Value> {
    lines_logged_for_stop(summary, StopReason::KeyReleased)
}

fn lines_logged_for_stop(summary: &Summary, stopped_because: StopReason) -> Vec<Value> {
    lines_logged(&FinishedDictation {
        summary: summary.clone(),
        stopped_because,
        audio: None,
        transcription: Transcription::NotAttempted,
    })
}

fn lines_logged(dictation: &FinishedDictation) -> Vec<Value> {
    let log_dir = tempfile::tempdir().unwrap();
    let (subscriber, guard) = file_subscriber(log_dir.path()).unwrap();
    tracing::subscriber::with_default(subscriber, || record_dictation(dictation));
    drop(guard);
    common::read_lines(log_dir.path())
}

#[test]
fn a_dictation_is_logged_as_one_line_with_its_id_and_latencies_in_milliseconds() {
    let mut timeline = Timeline::new(DictationId(7));
    timeline.mark(Mark::KeyDown, at_micros(1_000_000));
    timeline.mark(Mark::FirstSample, at_micros(1_180_250));
    timeline.mark(Mark::FirstSound, at_micros(1_250_500));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));
    timeline.mark(Mark::TextLanded, at_micros(4_612_500));

    let lines = lines_logged_for(&timeline.summary());

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["dictation_id"], 7);
    assert_eq!(lines[0]["key_held_ms"], 3000.0);
    assert_eq!(lines[0]["key_down_to_first_sample_ms"], 180.25);
    assert_eq!(lines[0]["key_down_to_first_sound_ms"], 250.5);
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

#[test]
fn the_line_says_how_much_audio_was_captured_and_how_much_was_dropped() {
    let lines = lines_logged(&FinishedDictation {
        summary: Timeline::new(DictationId(10)).summary(),
        stopped_because: StopReason::KeyReleased,
        audio: Some(AudioStats {
            duration: Duration::from_millis(500),
            dropped_samples: 7,
            peak: 1.0,
            rms: 0.1,
        }),
        transcription: Transcription::NotAttempted,
    });

    assert_eq!(lines[0]["captured_audio_ms"], 500.0);
    assert_eq!(lines[0]["dropped_samples"], 7);
}

#[test]
fn the_line_reports_a_transcription_without_the_words_that_were_said() {
    let mut timeline = Timeline::new(DictationId(11));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));
    timeline.mark(Mark::CaptureStopped, at_micros(4_025_000));
    timeline.mark(Mark::TranscriptReady, at_micros(4_437_500));

    let lines = lines_logged(&FinishedDictation {
        summary: timeline.summary(),
        stopped_because: StopReason::KeyReleased,
        audio: None,
        transcription: Transcription::Succeeded(Transcript {
            text: "hello world".to_string(),
        }),
    });

    assert_eq!(lines[0]["transcription"], "succeeded");
    assert_eq!(lines[0]["transcript_chars"], 11);
    assert_eq!(lines[0]["key_up_to_capture_stopped_ms"], 25.0);
    assert_eq!(lines[0]["transcription_ms"], 412.5);
    assert!(!lines[0].to_string().contains("hello"));
}

#[test]
fn the_line_says_why_a_transcription_failed() {
    let summary = Timeline::new(DictationId(12)).summary();

    for (failure, expected) in [
        (TranscriptionFailure::NotConfigured, "not_configured"),
        (TranscriptionFailure::Unauthorized, "unauthorized"),
        (TranscriptionFailure::Unavailable, "unavailable"),
        (TranscriptionFailure::Rejected, "rejected"),
        (TranscriptionFailure::Unreachable, "unreachable"),
    ] {
        let lines = lines_logged(&FinishedDictation {
            summary: summary.clone(),
            stopped_because: StopReason::KeyReleased,
            audio: None,
            transcription: Transcription::Failed(failure),
        });

        assert_eq!(lines[0]["transcription"], "failed");
        assert_eq!(lines[0]["transcription_failure"], expected);
    }
}
