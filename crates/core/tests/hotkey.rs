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

#[test]
fn a_release_that_was_missed_is_reported_once_on_resync() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);
    decoder.flags_changed(RIGHT_OPTION_DOWN);

    assert_eq!(
        decoder.resync(NO_MODIFIERS),
        Some(KeyEdge::Released(PressId(1)))
    );
    assert_eq!(decoder.flags_changed(NO_MODIFIERS), None);
}

#[test]
fn a_key_found_already_held_must_be_released_before_it_can_start_a_press() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);

    assert_eq!(decoder.resync(RIGHT_OPTION_DOWN), None);
    assert_eq!(
        decoder.flags_changed(RIGHT_OPTION_AND_RIGHT_SHIFT_DOWN),
        None
    );
    assert_eq!(decoder.flags_changed(NO_MODIFIERS), None);
    assert_eq!(
        decoder.flags_changed(RIGHT_OPTION_DOWN),
        Some(KeyEdge::Pressed(PressId(1)))
    );
}

#[test]
fn a_key_still_held_at_resync_stays_the_same_press() {
    let mut decoder = HotkeyDecoder::new(HotkeyKey::RightOption);
    decoder.flags_changed(RIGHT_OPTION_DOWN);

    assert_eq!(decoder.resync(RIGHT_OPTION_DOWN), None);
    assert_eq!(
        decoder.flags_changed(NO_MODIFIERS),
        Some(KeyEdge::Released(PressId(1)))
    );
}

// Flag words as macOS reports each key held on its own: the shared
// modifier bit, the key's own side bit, and the always-set 0x100.
const KEYS_AND_FLAGS: [(HotkeyKey, u64); 6] = [
    (HotkeyKey::RightOption, 0x80140),
    (HotkeyKey::LeftOption, 0x80120),
    (HotkeyKey::RightCommand, 0x100110),
    (HotkeyKey::RightControl, 0x42100),
    (HotkeyKey::LeftControl, 0x40101),
    (HotkeyKey::Fn, 0x800100),
];

#[test]
fn each_hotkey_is_pressed_by_its_own_key_and_no_other() {
    for (key, _) in KEYS_AND_FLAGS {
        for (other, flags) in KEYS_AND_FLAGS {
            let mut decoder = HotkeyDecoder::new(key);
            let edge = decoder.flags_changed(flags);
            if key == other {
                assert_eq!(edge, Some(KeyEdge::Pressed(PressId(1))), "{key:?}");
            } else {
                assert_eq!(edge, None, "{key:?} pressed by {other:?}");
            }
        }
    }
}

#[test]
fn each_hotkey_is_read_back_from_the_name_it_is_saved_under() {
    for (key, _) in KEYS_AND_FLAGS {
        assert_eq!(HotkeyKey::from_setting(Some(key.name())), key);
    }
}

#[test]
fn with_no_saved_hotkey_or_one_not_known_right_option_is_used() {
    assert_eq!(HotkeyKey::from_setting(None), HotkeyKey::RightOption);
    assert_eq!(
        HotkeyKey::from_setting(Some("caps_lock")),
        HotkeyKey::RightOption
    );
}
