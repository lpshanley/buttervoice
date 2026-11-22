use std::time::Duration;

use buttervoice_core::capture::{
    AudioFormat, CapturedAudio, InputKind, capture_channel, choose_input, input_kind,
};
use buttervoice_core::timeline::Timestamp;

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
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

#[test]
fn only_the_macs_own_microphone_counts_as_built_in() {
    assert_eq!(input_kind(BUILT_IN), InputKind::BuiltIn);
    assert_eq!(input_kind(HEADSET), InputKind::Other);
}

const ONE_SAMPLE_PER_MS: AudioFormat = AudioFormat {
    sample_rate: 1_000,
    channels: 1,
};

#[test]
fn captured_audio_comes_out_in_the_order_it_was_written() {
    let (mut writer, reader) = capture_channel(
        InputKind::Other,
        ONE_SAMPLE_PER_MS,
        Duration::from_millis(8),
    );
    writer.buffer_captured(&[0.1, 0.2], at_micros(1_000));
    writer.buffer_captured(&[0.3], at_micros(3_000));

    let audio = reader.finish().audio;

    assert_eq!(audio.samples, [0.1, 0.2, 0.3]);
    assert_eq!(audio.dropped_samples, 0);
}

#[test]
fn audio_that_does_not_fit_is_counted_as_dropped() {
    let (mut writer, reader) = capture_channel(
        InputKind::Other,
        ONE_SAMPLE_PER_MS,
        Duration::from_millis(4),
    );
    writer.buffer_captured(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], at_micros(1_000));

    let audio = reader.finish().audio;

    assert_eq!(audio.samples, [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(audio.dropped_samples, 2);
}

#[test]
fn collecting_makes_room_for_more_audio() {
    let (mut writer, mut reader) = capture_channel(
        InputKind::Other,
        ONE_SAMPLE_PER_MS,
        Duration::from_millis(4),
    );
    writer.buffer_captured(&[0.1, 0.2, 0.3, 0.4], at_micros(1_000));
    reader.collect();
    writer.buffer_captured(&[0.5, 0.6, 0.7, 0.8], at_micros(5_000));

    let audio = reader.finish().audio;

    assert_eq!(audio.samples, [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]);
    assert_eq!(audio.dropped_samples, 0);
}

#[test]
fn the_channel_reports_when_the_first_sample_and_first_sound_were_captured() {
    let (mut writer, reader) = capture_channel(
        InputKind::Other,
        ONE_SAMPLE_PER_MS,
        Duration::from_millis(8),
    );
    writer.buffer_captured(&[0.0, 0.0], at_micros(1_000));
    writer.buffer_captured(&[0.0, 0.25], at_micros(3_000));

    let report = reader.finish().report;

    assert_eq!(report.first_sample_at, Some(at_micros(1_000)));
    assert_eq!(report.first_sound_at, Some(at_micros(3_000)));
}

#[test]
fn captured_audio_says_which_kind_of_input_it_came_from() {
    let (_writer, reader) = capture_channel(
        InputKind::BuiltIn,
        ONE_SAMPLE_PER_MS,
        Duration::from_millis(8),
    );

    assert_eq!(reader.finish().input, InputKind::BuiltIn);
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

fn mono(samples: &[f32]) -> CapturedAudio {
    CapturedAudio {
        format: AudioFormat {
            sample_rate: 48_000,
            channels: 1,
        },
        samples: samples.to_vec(),
        dropped_samples: 0,
    }
}

#[test]
fn the_peak_is_the_loudest_sample_whichever_way_it_swings() {
    assert_eq!(mono(&[0.25, -0.75, 0.5]).stats().peak, 0.75);
}

#[test]
fn the_rms_level_is_the_root_of_the_mean_square() {
    // sqrt((0.25 + 0.25 + 0 + 0) / 4) = sqrt(1/8)
    let rms = mono(&[0.5, -0.5, 0.0, 0.0]).stats().rms;

    assert!((rms - 0.353_553_4).abs() < 1e-6, "rms was {rms}");
}

#[test]
fn audio_with_no_samples_has_a_level_of_zero() {
    let stats = mono(&[]).stats();

    assert_eq!(stats.peak, 0.0);
    assert_eq!(stats.rms, 0.0);
}

#[test]
fn quiet_audio_can_be_scaled_up_to_a_chosen_peak() {
    let louder = mono(&[0.125, -0.0625, 0.0]).with_peak_at(0.5);

    assert_eq!(louder.samples, [0.5, -0.25, 0.0]);
}

#[test]
fn pure_silence_is_left_alone_when_scaling() {
    let silence = mono(&[0.0, 0.0]).with_peak_at(0.5);

    assert_eq!(silence.samples, [0.0, 0.0]);
}

#[test]
fn audio_sent_for_transcription_peaks_three_decibels_below_full_scale() {
    let boosted = mono(&[0.03, -0.06, 0.0]).boosted_for_transcription();

    // 10^(-3/20)
    let peak = boosted.stats().peak;
    assert!((peak - 0.707_946).abs() < 1e-4, "peak was {peak}");
    assert_eq!(boosted.samples[2], 0.0);
}
