use buttervoice_samples::diff::{Part, word_diff};

fn same(word: &str) -> Part {
    Part::Same(word.to_string())
}

#[test]
fn words_that_match_apart_from_case_and_punctuation_are_the_same() {
    assert_eq!(
        word_diff("That's everything, see you.", "that’s Everything see you"),
        vec![
            same("That's"),
            same("everything,"),
            same("see"),
            same("you.")
        ]
    );
}

#[test]
fn a_word_heard_wrongly_is_shown_with_what_was_said_and_what_was_heard() {
    assert_eq!(
        word_diff("On it.", "Run it."),
        vec![
            Part::Changed {
                said: "On".to_string(),
                heard: "Run".to_string()
            },
            same("it.")
        ]
    );
}

#[test]
fn a_word_the_transcript_dropped_is_missing() {
    assert_eq!(
        word_diff("grab a coffee", "grab coffee"),
        vec![same("grab"), Part::Missing("a".to_string()), same("coffee")]
    );
}

#[test]
fn a_word_the_transcript_added_is_extra() {
    assert_eq!(
        word_diff("recommends a way", "recommends them a way"),
        vec![
            same("recommends"),
            Part::Extra("them".to_string()),
            same("a"),
            same("way")
        ]
    );
}

#[test]
fn hearing_nothing_misses_every_word() {
    assert_eq!(
        word_diff("not yet", ""),
        vec![
            Part::Missing("not".to_string()),
            Part::Missing("yet".to_string())
        ]
    );
}

#[test]
fn text_written_over_silence_is_all_extra() {
    assert_eq!(
        word_diff("", "Thank you."),
        vec![
            Part::Extra("Thank".to_string()),
            Part::Extra("you.".to_string())
        ]
    );
}
