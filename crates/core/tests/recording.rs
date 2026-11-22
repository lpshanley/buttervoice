use buttervoice_core::DictationId;
use buttervoice_core::recording::{Decision, Mode, PressId, RecordingControl, StopReason};

#[test]
fn pressing_the_key_while_idle_starts_a_dictation() {
    let mut control = RecordingControl::new(Mode::PushToTalk);

    assert_eq!(
        control.key_pressed(PressId(1)),
        Decision::Start(DictationId(1))
    );
}

#[test]
fn releasing_the_key_stops_the_dictation_exactly_once() {
    let mut control = RecordingControl::new(Mode::PushToTalk);
    control.key_pressed(PressId(1));

    assert_eq!(
        control.key_released(PressId(1)),
        Decision::Stop(DictationId(1), StopReason::KeyReleased)
    );
    assert_eq!(control.key_released(PressId(1)), Decision::Ignore);
}

#[test]
fn a_press_is_ignored_while_the_previous_dictation_is_still_being_processed() {
    let mut control = RecordingControl::new(Mode::PushToTalk);
    control.key_pressed(PressId(1));
    control.key_released(PressId(1));

    assert_eq!(control.key_pressed(PressId(2)), Decision::Ignore);
}

#[test]
fn a_press_starts_the_next_dictation_once_the_previous_one_has_been_processed() {
    let mut control = RecordingControl::new(Mode::PushToTalk);
    control.key_pressed(PressId(1));
    control.key_released(PressId(1));
    control.processing_finished();

    assert_eq!(
        control.key_pressed(PressId(2)),
        Decision::Start(DictationId(2))
    );
}

#[test]
fn a_repeated_processed_signal_does_not_abandon_the_dictation_being_recorded() {
    let mut control = RecordingControl::new(Mode::PushToTalk);
    control.key_pressed(PressId(1));
    control.key_released(PressId(1));
    control.processing_finished();
    control.key_pressed(PressId(2));
    control.processing_finished();

    assert_eq!(
        control.key_released(PressId(2)),
        Decision::Stop(DictationId(2), StopReason::KeyReleased)
    );
}

#[test]
fn a_late_release_of_an_earlier_press_cannot_stop_a_newer_dictation() {
    let mut control = RecordingControl::new(Mode::PushToTalk);
    control.key_pressed(PressId(1));
    control.key_released(PressId(1));
    control.processing_finished();
    control.key_pressed(PressId(2));

    assert_eq!(control.key_released(PressId(1)), Decision::Ignore);
    assert_eq!(
        control.key_released(PressId(2)),
        Decision::Stop(DictationId(2), StopReason::KeyReleased)
    );
}

#[test]
fn in_toggle_mode_releasing_the_key_does_not_stop_the_dictation() {
    let mut control = RecordingControl::new(Mode::Toggle);
    control.key_pressed(PressId(1));

    assert_eq!(control.key_released(PressId(1)), Decision::Ignore);
}

#[test]
fn in_toggle_mode_the_next_press_stops_the_dictation() {
    let mut control = RecordingControl::new(Mode::Toggle);
    control.key_pressed(PressId(1));
    control.key_released(PressId(1));

    assert_eq!(
        control.key_pressed(PressId(2)),
        Decision::Stop(DictationId(1), StopReason::KeyPressedAgain)
    );
}

#[test]
fn in_toggle_mode_a_repeat_of_the_starting_press_does_not_stop_the_dictation() {
    let mut control = RecordingControl::new(Mode::Toggle);
    control.key_pressed(PressId(1));

    assert_eq!(control.key_pressed(PressId(1)), Decision::Ignore);
}

#[test]
fn a_manual_stop_ends_the_dictation_and_the_later_key_release_does_nothing() {
    let mut control = RecordingControl::new(Mode::PushToTalk);
    control.key_pressed(PressId(1));

    assert_eq!(
        control.manual_stop(),
        Decision::Stop(DictationId(1), StopReason::Manual)
    );
    assert_eq!(control.key_released(PressId(1)), Decision::Ignore);
}

#[test]
fn losing_the_key_listener_stops_the_dictation_being_recorded() {
    let mut control = RecordingControl::new(Mode::PushToTalk);
    control.key_pressed(PressId(1));

    assert_eq!(
        control.key_listener_lost(),
        Decision::Stop(DictationId(1), StopReason::KeyListenerLost)
    );
}
