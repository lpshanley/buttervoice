use buttervoice_core::language::Language;

#[test]
fn a_mac_set_to_any_kind_of_portuguese_is_shown_brazilian_portuguese() {
    for system in ["pt-BR", "pt_BR", "pt-PT", "pt"] {
        assert_eq!(
            Language::closest_to(system),
            Language::BrazilianPortuguese,
            "{system}"
        );
    }
}

#[test]
fn a_mac_set_to_english_is_shown_english() {
    for system in ["en-US", "en_GB", "en"] {
        assert_eq!(Language::closest_to(system), Language::English, "{system}");
    }
}

#[test]
fn a_mac_set_to_a_language_the_app_does_not_have_is_shown_english() {
    for system in ["de-DE", "ptx", ""] {
        assert_eq!(Language::closest_to(system), Language::English, "{system}");
    }
}
