/// A language a person dictates in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    BrazilianPortuguese,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::BrazilianPortuguese];

    /// The BCP 47 tag the language is saved and logged under.
    pub fn tag(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::BrazilianPortuguese => "pt-BR",
        }
    }

    /// The language to show someone who has not chosen one, from the locale
    /// their Mac reports, such as `pt_BR` or `en-GB`.
    pub fn closest_to(system_locale: &str) -> Language {
        let primary = system_locale.split(['-', '_']).next().unwrap_or_default();
        if primary.eq_ignore_ascii_case("pt") {
            Language::BrazilianPortuguese
        } else {
            Language::English
        }
    }

    /// `None` when nothing is saved or the tag is not one this version
    /// supports. A language is the person's choice, so none is assumed here.
    pub fn from_setting(saved: Option<&str>) -> Option<Language> {
        Language::ALL
            .into_iter()
            .find(|language| Some(language.tag()) == saved)
    }
}
