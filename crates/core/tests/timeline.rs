use std::time::Duration;

use buttervoice_core::DictationId;
use buttervoice_core::timeline::{Mark, Timeline, Timestamp};

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

#[test]
fn reports_time_from_key_down_to_first_captured_sample() {
    let mut timeline = Timeline::new(DictationId(1));
    timeline.mark(Mark::KeyDown, at_micros(1_000_000));
    timeline.mark(Mark::FirstSample, at_micros(1_180_250));

    assert_eq!(
        timeline.summary().key_down_to_first_sample,
        Some(Duration::from_micros(180_250))
    );
}

#[test]
fn reports_no_wait_when_audio_was_already_being_captured_at_key_down() {
    let mut timeline = Timeline::new(DictationId(1));
    timeline.mark(Mark::FirstSample, at_micros(999_400));
    timeline.mark(Mark::KeyDown, at_micros(1_000_000));

    assert_eq!(
        timeline.summary().key_down_to_first_sample,
        Some(Duration::ZERO)
    );
}

#[test]
fn reports_time_from_key_up_to_the_text_being_typed() {
    let mut timeline = Timeline::new(DictationId(1));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));
    timeline.mark(Mark::TextTyped, at_micros(4_612_500));

    assert_eq!(
        timeline.summary().key_up_to_text_typed,
        Some(Duration::from_micros(612_500))
    );
}

#[test]
fn reports_how_long_the_key_was_held() {
    let mut timeline = Timeline::new(DictationId(1));
    timeline.mark(Mark::KeyDown, at_micros(1_000_000));
    timeline.mark(Mark::KeyUp, at_micros(1_956_365));

    assert_eq!(
        timeline.summary().key_held,
        Some(Duration::from_micros(956_365))
    );
}

#[test]
fn reports_time_from_key_down_to_the_first_real_sound() {
    let mut timeline = Timeline::new(DictationId(1));
    timeline.mark(Mark::KeyDown, at_micros(1_000_000));
    timeline.mark(Mark::FirstSample, at_micros(1_070_000));
    timeline.mark(Mark::FirstSound, at_micros(1_823_864));

    assert_eq!(
        timeline.summary().key_down_to_first_sound,
        Some(Duration::from_micros(823_864))
    );
}

#[test]
fn reports_how_long_stopping_capture_and_transcribing_took() {
    let mut timeline = Timeline::new(DictationId(1));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));
    timeline.mark(Mark::CaptureStopped, at_micros(4_025_000));
    timeline.mark(Mark::TranscriptReady, at_micros(4_437_500));

    let summary = timeline.summary();

    assert_eq!(
        summary.key_up_to_capture_stopped,
        Some(Duration::from_micros(25_000))
    );
    assert_eq!(summary.transcription, Some(Duration::from_micros(412_500)));
}
