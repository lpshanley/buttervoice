use std::time::Duration;

use buttervoice_core::capture::{
    AudioFormat, CaptureMarks, CapturedAudio, capture_channel, choose_input,
};
use buttervoice_core::timeline::Timestamp;

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

#[test]
fn the_first_buffer_marks_the_first_sample() {
    let marks = CaptureMarks::new();
    marks.buffer_captured(&[0.0, 0.0], at_micros(1_070_000));
    marks.buffer_captured(&[0.0, 0.0], at_micros(1_090_000));

    assert_eq!(marks.report().first_sample_at, Some(at_micros(1_070_000)));
}

#[test]
fn the_first_sound_is_the_first_buffer_that_is_not_silent() {
    let marks = CaptureMarks::new();
    marks.buffer_captured(&[0.0, 0.0], at_micros(1_070_000));
    marks.buffer_captured(&[0.0, -0.002], at_micros(1_090_000));
    marks.buffer_captured(&[0.5, 0.5], at_micros(1_110_000));

    assert_eq!(marks.report().first_sound_at, Some(at_micros(1_090_000)));
}

// The built-in id is what macOS reports on a MacBook Pro. The headset id is
// made up, in the shape macOS gives a Bluetooth input.
const BUILT_IN: &str = "BuiltInMicrophoneDevice";
const HEADSET: &str = "00-11-22-33-44-55:input";

#[test]
fn the_built_in_microphone_is_preferred_over_the_system_default() {
    assert_eq!(
        choose_input(&[HEADSET, BUILT_IN], Some(HEADSET)),
        Some(BUILT_IN)
    );
}

#[test]
fn without_a_built_in_microphone_the_system_default_is_used() {
    assert_eq!(choose_input(&[HEADSET], Some(HEADSET)), Some(HEADSET));
}

const ONE_SAMPLE_PER_MS: AudioFormat = AudioFormat {
    sample_rate: 1_000,
    channels: 1,
};

#[test]
fn captured_audio_comes_out_in_the_order_it_was_written() {
    let (mut writer, reader) = capture_channel(ONE_SAMPLE_PER_MS, Duration::from_millis(8));
    writer.buffer_captured(&[0.1, 0.2], at_micros(1_000));
    writer.buffer_captured(&[0.3], at_micros(3_000));

    let (audio, _) = reader.finish();

    assert_eq!(audio.samples, [0.1, 0.2, 0.3]);
    assert_eq!(audio.dropped_samples, 0);
}

#[test]
fn audio_that_does_not_fit_is_counted_as_dropped() {
    let (mut writer, reader) = capture_channel(ONE_SAMPLE_PER_MS, Duration::from_millis(4));
    writer.buffer_captured(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], at_micros(1_000));

    let (audio, _) = reader.finish();

    assert_eq!(audio.samples, [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(audio.dropped_samples, 2);
}

#[test]
fn collecting_makes_room_for_more_audio() {
    let (mut writer, mut reader) = capture_channel(ONE_SAMPLE_PER_MS, Duration::from_millis(4));
    writer.buffer_captured(&[0.1, 0.2, 0.3, 0.4], at_micros(1_000));
    reader.collect();
    writer.buffer_captured(&[0.5, 0.6, 0.7, 0.8], at_micros(5_000));

    let (audio, _) = reader.finish();

    assert_eq!(audio.samples, [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]);
    assert_eq!(audio.dropped_samples, 0);
}

#[test]
fn the_channel_reports_when_the_first_sample_and_first_sound_were_captured() {
    let (mut writer, reader) = capture_channel(ONE_SAMPLE_PER_MS, Duration::from_millis(8));
    writer.buffer_captured(&[0.0, 0.0], at_micros(1_000));
    writer.buffer_captured(&[0.0, 0.25], at_micros(3_000));

    let (_, report) = reader.finish();

    assert_eq!(report.first_sample_at, Some(at_micros(1_000)));
    assert_eq!(report.first_sound_at, Some(at_micros(3_000)));
}

#[test]
fn captured_audio_knows_how_long_it_lasts() {
    let stereo = CapturedAudio {
        format: AudioFormat {
            sample_rate: 48_000,
            channels: 2,
        },
        samples: vec![0.0; 48_000],
        dropped_samples: 0,
    };

    assert_eq!(stereo.duration(), Duration::from_millis(500));
}
