use std::time::Duration;

use buttervoice_core::DictationId;
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::KeyDictations;
use buttervoice_core::recording::{Mode, StopReason};
use buttervoice_core::timeline::Timestamp;

const RIGHT_OPTION_DOWN: u64 = 0x80140;
const NO_MODIFIERS: u64 = 0x100;

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

#[test]
fn holding_and_releasing_the_key_finishes_one_dictation() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);

    assert_eq!(
        dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000)),
        None
    );
    let finished = dictations
        .flags_changed(NO_MODIFIERS, at_micros(1_956_365))
        .expect("releasing the key should finish the dictation");

    assert_eq!(finished.summary.dictation_id, DictationId(1));
    assert_eq!(
        finished.summary.key_held,
        Some(Duration::from_micros(956_365))
    );
    assert_eq!(finished.stopped_because, StopReason::KeyReleased);
}

#[test]
fn a_release_missed_by_the_listener_finishes_the_dictation_on_resync() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));

    let finished = dictations
        .resync(NO_MODIFIERS, at_micros(1_500_000))
        .expect("finding the key up should finish the dictation");

    assert_eq!(
        finished.summary.key_held,
        Some(Duration::from_micros(500_000))
    );
    assert_eq!(finished.stopped_because, StopReason::KeyReleased);
}

#[test]
fn losing_the_key_listener_finishes_the_dictation_and_says_so() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));

    let finished = dictations
        .key_listener_lost(at_micros(1_250_000))
        .expect("losing the listener should finish the dictation");

    assert_eq!(finished.stopped_because, StopReason::KeyListenerLost);
}
