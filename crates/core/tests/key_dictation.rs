use std::time::Duration;

use buttervoice_core::DictationId;
use buttervoice_core::capture::CaptureReport;
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{CaptureCommand, KeyDictations};
use buttervoice_core::recording::{Mode, StopReason};
use buttervoice_core::timeline::Timestamp;

const RIGHT_OPTION_DOWN: u64 = 0x80140;
const NO_MODIFIERS: u64 = 0x100;

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

fn push_to_talk() -> KeyDictations {
    KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk)
}

#[test]
fn pressing_the_key_starts_capture_and_releasing_it_stops_capture() {
    let mut dictations = push_to_talk();

    assert_eq!(
        dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000)),
        Some(CaptureCommand::Start)
    );
    assert_eq!(
        dictations.flags_changed(NO_MODIFIERS, at_micros(1_956_365)),
        Some(CaptureCommand::Stop)
    );
}

#[test]
fn a_dictation_finishes_once_capture_has_stopped() {
    let mut dictations = push_to_talk();
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));
    dictations.flags_changed(NO_MODIFIERS, at_micros(1_956_365));

    let finished = dictations
        .capture_stopped(CaptureReport {
            first_sample_at: Some(at_micros(1_084_000)),
            first_sound_at: Some(at_micros(1_095_000)),
        })
        .expect("stopping capture should finish the dictation");

    assert_eq!(finished.summary.dictation_id, DictationId(1));
    assert_eq!(finished.stopped_because, StopReason::KeyReleased);
    assert_eq!(
        finished.summary.key_held,
        Some(Duration::from_micros(956_365))
    );
    assert_eq!(
        finished.summary.key_down_to_first_sample,
        Some(Duration::from_micros(84_000))
    );
    assert_eq!(
        finished.summary.key_down_to_first_sound,
        Some(Duration::from_micros(95_000))
    );
}

#[test]
fn a_new_press_is_ignored_until_capture_has_stopped() {
    let mut dictations = push_to_talk();
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));
    dictations.flags_changed(NO_MODIFIERS, at_micros(1_500_000));

    assert_eq!(
        dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_600_000)),
        None
    );
}

#[test]
fn a_release_missed_by_the_listener_stops_capture_on_resync() {
    let mut dictations = push_to_talk();
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));

    assert_eq!(
        dictations.resync(NO_MODIFIERS, at_micros(1_500_000)),
        Some(CaptureCommand::Stop)
    );
    let finished = dictations
        .capture_stopped(CaptureReport::default())
        .expect("stopping capture should finish the dictation");
    assert_eq!(
        finished.summary.key_held,
        Some(Duration::from_micros(500_000))
    );
    assert_eq!(finished.stopped_because, StopReason::KeyReleased);
}

#[test]
fn losing_the_key_listener_stops_capture_and_the_dictation_says_why() {
    let mut dictations = push_to_talk();
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));

    assert_eq!(
        dictations.key_listener_lost(at_micros(1_250_000)),
        Some(CaptureCommand::Stop)
    );
    let finished = dictations
        .capture_stopped(CaptureReport::default())
        .expect("stopping capture should finish the dictation");
    assert_eq!(finished.stopped_because, StopReason::KeyListenerLost);
}
