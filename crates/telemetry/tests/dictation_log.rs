mod common;

use std::time::Duration;

use buttervoice_core::DictationId;
use buttervoice_core::capture::{AudioFormat, AudioStats, InputKind};
use buttervoice_core::key_dictation::FinishedDictation;
use buttervoice_core::recording::StopReason;
use buttervoice_core::timeline::{Mark, Summary, Timeline, Timestamp};
use buttervoice_core::transcription::{Provider, Transcript, Transcription, TranscriptionFailure};
use buttervoice_core::typing::{Typing, TypingFailure};
use buttervoice_telemetry::{file_subscriber, record_dictation, record_dictation_started};
use serde_json::Value;

const MONO_48K: AudioFormat = AudioFormat {
    sample_rate: 48_000,
    channels: 1,
};

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

fn lines_logged_for(summary: &Summary) -> Vec<Value> {
    lines_logged_for_stop(summary, StopReason::KeyReleased)
}

fn lines_logged_for_stop(summary: &Summary, stopped_because: StopReason) -> Vec<Value> {
    lines_logged(&FinishedDictation {
        stopped_because,
        ..uneventful(summary.clone())
    })
}

/// A dictation in which nothing was captured, transcribed or typed. Each test
/// overrides the part it is about.
fn uneventful(summary: Summary) -> FinishedDictation {
    FinishedDictation {
        summary,
        stopped_because: StopReason::KeyReleased,
        audio: None,
        input: None,
        provider: None,
        transcription: Transcription::NotAttempted,
        typing: Typing::NotAttempted,
    }
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
    timeline.mark(Mark::TextTyped, at_micros(4_612_500));

    let lines = lines_logged_for(&timeline.summary());

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["dictation_id"], 7);
    assert_eq!(lines[0]["key_held_ms"], 3000.0);
    assert_eq!(lines[0]["key_down_to_first_sample_ms"], 180.25);
    assert_eq!(lines[0]["key_down_to_first_sound_ms"], 250.5);
    assert_eq!(lines[0]["key_up_to_text_typed_ms"], 612.5);
}

#[test]
fn a_latency_that_was_never_measured_is_left_out_of_the_line() {
    let mut timeline = Timeline::new(DictationId(8));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));

    let lines = lines_logged_for(&timeline.summary());

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].get("key_up_to_text_typed_ms"), None);
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
        audio: Some(AudioStats {
            format: MONO_48K,
            duration: Duration::from_millis(500),
            dropped_samples: 7,
            peak: 1.0,
            rms: 0.1,
        }),
        ..uneventful(Timeline::new(DictationId(10)).summary())
    });

    assert_eq!(lines[0]["captured_audio_ms"], 500.0);
    assert_eq!(lines[0]["dropped_samples"], 7);
    assert_eq!(lines[0]["audio_peak_dbfs"], 0.0);
    assert_eq!(lines[0]["audio_rms_dbfs"], -20.0);
}

#[test]
fn the_line_reports_a_transcription_without_the_words_that_were_said() {
    let mut timeline = Timeline::new(DictationId(11));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));
    timeline.mark(Mark::CaptureStopped, at_micros(4_025_000));
    timeline.mark(Mark::TranscriptReady, at_micros(4_437_500));

    let lines = lines_logged(&FinishedDictation {
        transcription: Transcription::Succeeded(Transcript {
            text: "hello world".to_string(),
        }),
        ..uneventful(timeline.summary())
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
        (TranscriptionFailure::EngineFailed, "engine_failed"),
    ] {
        let lines = lines_logged(&FinishedDictation {
            transcription: Transcription::Failed(failure),
            ..uneventful(summary.clone())
        });

        assert_eq!(lines[0]["transcription"], "failed");
        assert_eq!(lines[0]["transcription_failure"], expected);
    }
}

#[test]
fn audio_that_is_pure_silence_has_no_level_in_the_line() {
    let lines = lines_logged(&FinishedDictation {
        audio: Some(AudioStats {
            format: MONO_48K,
            duration: Duration::from_millis(500),
            dropped_samples: 0,
            peak: 0.0,
            rms: 0.0,
        }),
        ..uneventful(Timeline::new(DictationId(13)).summary())
    });

    assert_eq!(lines[0].get("audio_peak_dbfs"), None);
    assert_eq!(lines[0].get("audio_rms_dbfs"), None);
}

#[test]
fn the_line_says_whether_the_text_was_typed() {
    let dictation = |typing| FinishedDictation {
        typing,
        ..uneventful(Timeline::new(DictationId(14)).summary())
    };

    let typed = lines_logged(&dictation(Typing::Typed));
    assert_eq!(typed[0]["typing"], "typed");
    assert_eq!(typed[0].get("typing_failure"), None);

    let not_attempted = lines_logged(&dictation(Typing::NotAttempted));
    assert_eq!(not_attempted[0]["typing"], "not_attempted");

    let refused = lines_logged(&dictation(Typing::Failed(TypingFailure::NotPermitted)));
    assert_eq!(refused[0]["typing"], "failed");
    assert_eq!(refused[0]["typing_failure"], "not_permitted");
}

#[test]
fn the_line_says_which_kind_of_input_was_used_and_its_format() {
    for (input, expected) in [
        (InputKind::BuiltIn, "built_in"),
        (InputKind::Other, "other"),
    ] {
        let lines = lines_logged(&FinishedDictation {
            audio: Some(AudioStats {
                format: AudioFormat {
                    sample_rate: 24_000,
                    channels: 2,
                },
                duration: Duration::from_millis(500),
                dropped_samples: 0,
                peak: 0.5,
                rms: 0.1,
            }),
            input: Some(input),
            ..uneventful(Timeline::new(DictationId(15)).summary())
        });

        assert_eq!(lines[0]["input"], expected);
        assert_eq!(lines[0]["input_sample_rate_hz"], 24_000);
        assert_eq!(lines[0]["input_channels"], 2);
    }
}

#[test]
fn the_line_says_which_provider_transcribed_the_dictation() {
    for (provider, expected) in [(Provider::Local, "local"), (Provider::Grok, "grok")] {
        let lines = lines_logged(&FinishedDictation {
            provider: Some(provider),
            ..uneventful(Timeline::new(DictationId(16)).summary())
        });

        assert_eq!(lines[0]["provider"], expected);
    }
}

#[test]
fn the_start_of_a_dictation_is_logged_with_its_id() {
    let log_dir = tempfile::tempdir().unwrap();
    let (subscriber, guard) = file_subscriber(log_dir.path()).unwrap();
    tracing::subscriber::with_default(subscriber, || record_dictation_started(DictationId(7)));
    drop(guard);

    let lines = common::read_lines(log_dir.path());
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["message"], "dictation started");
    assert_eq!(lines[0]["dictation_id"], 7);
}
