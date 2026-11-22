use buttervoice_core::capture::{CaptureMarks, choose_input};
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
