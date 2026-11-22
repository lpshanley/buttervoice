use buttervoice_core::hotkey::{HotkeyDecoder, HotkeyKey, KeyEdge};
use buttervoice_core::recording::PressId;

// Flag words captured from a real Right Option press and release on macOS.
const RIGHT_OPTION_DOWN: u64 = 0x80140;
const NO_MODIFIERS: u64 = 0x100;

#[test]
fn pressing_right_option_is_reported_as_a_press() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);

    assert_eq!(
        decoder.flags_changed(RIGHT_OPTION_DOWN),
        Some(KeyEdge::Pressed(PressId(1)))
    );
}

#[test]
fn releasing_right_option_is_reported_as_the_release_of_that_press() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);
    decoder.flags_changed(RIGHT_OPTION_DOWN);

    assert_eq!(
        decoder.flags_changed(NO_MODIFIERS),
        Some(KeyEdge::Released(PressId(1)))
    );
}

#[test]
fn each_press_of_the_key_gets_its_own_id() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);
    decoder.flags_changed(RIGHT_OPTION_DOWN);
    decoder.flags_changed(NO_MODIFIERS);

    assert_eq!(
        decoder.flags_changed(RIGHT_OPTION_DOWN),
        Some(KeyEdge::Pressed(PressId(2)))
    );
    assert_eq!(
        decoder.flags_changed(NO_MODIFIERS),
        Some(KeyEdge::Released(PressId(2)))
    );
}

// The remaining flag words are not captured: they are built from the per-key
// masks in macOS's IOLLEvent.h (left Option 0x20, right Option 0x40, right
// Shift 0x4) and the shared masks (Option 0x80000, Shift 0x20000).
const RIGHT_OPTION_AND_RIGHT_SHIFT_DOWN: u64 = 0xA0144;

#[test]
fn another_modifier_changing_while_the_key_is_held_is_not_a_second_press() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);
    decoder.flags_changed(RIGHT_OPTION_DOWN);

    assert_eq!(
        decoder.flags_changed(RIGHT_OPTION_AND_RIGHT_SHIFT_DOWN),
        None
    );
}
const LEFT_OPTION_DOWN: u64 = 0x80120;
const BOTH_OPTIONS_DOWN: u64 = 0x80160;

#[test]
fn releasing_right_option_while_left_option_is_still_held_is_a_release() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);
    decoder.flags_changed(LEFT_OPTION_DOWN);
    decoder.flags_changed(BOTH_OPTIONS_DOWN);

    assert_eq!(
        decoder.flags_changed(LEFT_OPTION_DOWN),
        Some(KeyEdge::Released(PressId(1)))
    );
    assert_eq!(decoder.flags_changed(NO_MODIFIERS), None);
}
