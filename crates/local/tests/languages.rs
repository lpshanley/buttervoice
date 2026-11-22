use buttervoice_core::language::Language;
use buttervoice_local::speaks;

fn listing(codes: &[&str]) -> Vec<String> {
    codes.iter().map(|code| code.to_string()).collect()
}

#[test]
fn an_english_only_model_does_not_speak_portuguese() {
    let english_only = listing(&["en"]);

    assert!(speaks(&english_only, Language::English));
    assert!(!speaks(&english_only, Language::BrazilianPortuguese));
}

#[test]
fn a_model_that_lists_portuguese_speaks_the_brazilian_kind() {
    let multilingual = listing(&["de", "en", "pt"]);

    assert!(speaks(&multilingual, Language::BrazilianPortuguese));
    assert!(speaks(&multilingual, Language::English));
}

#[test]
fn a_model_that_lists_no_languages_is_not_tied_to_any() {
    assert!(speaks(&[], Language::BrazilianPortuguese));
}
