use buttervoice_replay::scoring::word_error_rate;

#[test]
fn the_same_words_score_zero() {
    assert_eq!(
        word_error_rate("turn the lights off", "turn the lights off"),
        0.0
    );
}

#[test]
fn case_and_punctuation_do_not_count() {
    assert_eq!(
        word_error_rate("Hello, world. It's late!", "hello world it's late"),
        0.0
    );
}

#[test]
fn one_wrong_word_in_four_scores_a_quarter() {
    assert_eq!(
        word_error_rate("turn the lights off", "turn the light off"),
        0.25
    );
}

#[test]
fn a_missed_word_counts_as_an_error() {
    assert_eq!(
        word_error_rate("turn the lights off", "turn lights off"),
        0.25
    );
}

#[test]
fn an_added_word_counts_as_an_error() {
    assert_eq!(
        word_error_rate("turn the lights off", "turn the lights off now"),
        0.25
    );
}

#[test]
fn hearing_nothing_scores_one() {
    assert_eq!(word_error_rate("turn the lights off", ""), 1.0);
}

#[test]
fn a_curly_apostrophe_matches_a_straight_one() {
    assert_eq!(word_error_rate("it's late", "it\u{2019}s late"), 0.0);
}
