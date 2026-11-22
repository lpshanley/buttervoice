use buttervoice_core::typing::key_event_chunks;

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn short_text_fits_on_one_keyboard_event() {
    assert_eq!(key_event_chunks("hi there"), [utf16("hi there")]);
}

#[test]
fn long_text_is_split_into_events_of_at_most_twenty_units_in_order() {
    let text = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHI";

    assert_eq!(
        key_event_chunks(text),
        [
            utf16("abcdefghijklmnopqrst"),
            utf16("uvwxyz0123456789ABCD"),
            utf16("EFGHI"),
        ]
    );
}

#[test]
fn a_character_that_needs_two_units_is_never_split_across_events() {
    let text = format!("{}\u{1F600}b", "a".repeat(19));

    assert_eq!(
        key_event_chunks(&text),
        [
            utf16(&"a".repeat(19)),
            vec![0xD83D, 0xDE00, u16::from(b'b')]
        ]
    );
}

#[test]
fn no_text_means_no_events() {
    assert_eq!(key_event_chunks(""), Vec::<Vec<u16>>::new());
}
