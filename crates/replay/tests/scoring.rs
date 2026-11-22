use buttervoice_core::language::Language;
use buttervoice_replay::scoring::{word_error_rate, word_errors};

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

// A model writing figures for what was said in words has heard it right;
// only a different number is a mistake.

#[test]
fn a_time_said_in_words_matches_it_written_with_a_colon() {
    assert_eq!(
        word_error_rate("Ping me at nine thirty.", "Ping me at 9:30"),
        0.0
    );
}

#[test]
fn an_amount_said_in_words_matches_it_written_as_money() {
    assert_eq!(
        word_error_rate(
            "The invoice is for one thousand two hundred fifty dollars.",
            "The invoice is for $1,250."
        ),
        0.0
    );
}

#[test]
fn an_ordinal_said_in_words_matches_it_written_either_way() {
    assert_eq!(word_error_rate("due March third", "due March 3"), 0.0);
    assert_eq!(word_error_rate("due March third", "due March 3rd"), 0.0);
    assert_eq!(
        word_error_rate("on the twenty-first of October", "on the 21st of October"),
        0.0
    );
}

#[test]
fn a_decimal_said_in_words_matches_it_written_with_a_point() {
    assert_eq!(
        word_error_rate("version two point four", "version 2.4"),
        0.0
    );
}

#[test]
fn a_plain_number_said_in_words_matches_its_figures() {
    assert_eq!(
        word_error_rate("about ten minutes late", "about 10 minutes late"),
        0.0
    );
    assert_eq!(word_error_rate("twenty one people", "21 people"), 0.0);
}

#[test]
fn a_different_number_is_still_a_mistake() {
    assert_eq!(
        word_error_rate("Ping me at nine thirty", "Ping me at 9:45"),
        0.2
    );
}

#[test]
fn words_that_are_not_numbers_are_compared_as_before() {
    assert_eq!(word_error_rate("On it.", "Run it."), 0.5);
}

// Brazilian Portuguese writes numbers its own way: a dot for thousands, a
// comma for decimals, "9h30" for times, "1º" for ordinals, "R$" for money.

fn in_portuguese(said: &str, heard: &str) -> f64 {
    let (errors, words) = word_errors(Language::BrazilianPortuguese, said, heard);
    errors as f64 / words as f64
}

#[test]
fn a_portuguese_time_said_in_words_matches_it_written_either_way() {
    assert_eq!(
        in_portuguese("me chama às nove e meia", "me chama às 9:30"),
        0.0
    );
    assert_eq!(
        in_portuguese("me chama às nove e meia", "me chama às 9h30"),
        0.0
    );
    assert_eq!(in_portuguese("nove e quinze", "9:15"), 0.0);
    assert_eq!(in_portuguese("nove e cinco", "9:05"), 0.0);
}

#[test]
fn a_portuguese_hour_on_its_own_matches_it_written_with_an_h() {
    assert_eq!(in_portuguese("às nove horas", "às 9h"), 0.0);
}

#[test]
fn a_portuguese_decimal_said_with_a_comma_matches_it_written_with_one() {
    assert_eq!(in_portuguese("um vírgula cinco litro", "1,5 litro"), 0.0);
}

#[test]
fn a_portuguese_amount_said_in_words_matches_it_written_as_reais() {
    assert_eq!(
        in_portuguese(
            "a fatura é de mil duzentos e cinquenta reais",
            "a fatura é de R$ 1.250"
        ),
        0.0
    );
    assert_eq!(
        in_portuguese("mil duzentos e cinquenta reais", "R$1.250"),
        0.0
    );
}

#[test]
fn portuguese_dates_said_in_words_match_them_in_figures() {
    assert_eq!(in_portuguese("três de março", "3 de março"), 0.0);
    assert_eq!(in_portuguese("primeiro de março", "1º de março"), 0.0);
    assert_eq!(in_portuguese("vinte e um de outubro", "21 de outubro"), 0.0);
}

#[test]
fn a_portuguese_decimal_said_in_words_matches_it_written_with_a_point() {
    assert_eq!(in_portuguese("versão dois ponto quatro", "versão 2.4"), 0.0);
}

#[test]
fn and_between_two_whole_numbers_is_still_and() {
    assert_eq!(in_portuguese("um e dois", "1 e 2"), 0.0);
    assert!(in_portuguese("um e dois", "3") > 0.0);
}

#[test]
fn english_numbers_are_not_read_as_portuguese() {
    assert!(in_portuguese("nine thirty", "9:30") > 0.0);
    let (errors, _) = word_errors(Language::English, "nove e meia", "9:30");
    assert!(errors > 0);
}

#[test]
fn portuguese_number_words_in_their_other_forms_match_figures() {
    assert_eq!(
        in_portuguese("duzentas e cinquenta pessoas", "250 pessoas"),
        0.0
    );
    assert_eq!(in_portuguese("catorze", "14"), 0.0);
    assert_eq!(in_portuguese("quatorze", "14"), 0.0);
    assert_eq!(in_portuguese("cem", "100"), 0.0);
    assert_eq!(in_portuguese("cento e vinte", "120"), 0.0);
    assert_eq!(in_portuguese("um milhão e duzentos mil", "1.200.000"), 0.0);
}
