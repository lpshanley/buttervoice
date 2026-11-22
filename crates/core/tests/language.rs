use buttervoice_core::language::Language;

#[test]
fn each_language_is_saved_under_its_standard_tag() {
    assert_eq!(Language::English.tag(), "en");
    assert_eq!(Language::BrazilianPortuguese.tag(), "pt-BR");
}

#[test]
fn a_saved_tag_is_read_back_as_that_language() {
    assert_eq!(Language::from_setting(Some("en")), Some(Language::English));
    assert_eq!(
        Language::from_setting(Some("pt-BR")),
        Some(Language::BrazilianPortuguese)
    );
}

#[test]
fn nobody_is_given_a_language_they_did_not_choose() {
    assert_eq!(Language::from_setting(None), None);
    assert_eq!(Language::from_setting(Some("tlh")), None);
}

#[test]
fn every_supported_language_can_be_listed_for_a_person_to_choose_from() {
    assert_eq!(
        Language::ALL,
        [Language::English, Language::BrazilianPortuguese]
    );
}
