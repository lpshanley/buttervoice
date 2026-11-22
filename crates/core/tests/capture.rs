use buttervoice_core::capture::CaptureMarks;
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
