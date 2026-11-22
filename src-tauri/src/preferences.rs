//! The choices that outlive a run, kept as JSON in the app's own folder.

use std::path::Path;

use buttervoice_core::language::Language;
use buttervoice_core::transcription::Provider;
use serde_json::{Value, json};

/// The language a person dictates in and the language the app is shown in
/// are saved separately, so they can differ, even while one control sets both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preferences {
    pub provider: Provider,
    pub dictation_language: Option<Language>,
    pub display_language: Option<Language>,
}

const PROVIDER: &str = "provider";
const DICTATION_LANGUAGE: &str = "dictation_language";
const DISPLAY_LANGUAGE: &str = "display_language";

pub fn load(file: &Path) -> Preferences {
    let saved = std::fs::read_to_string(file)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok());
    let text = |name: &str| saved.as_ref()?.get(name)?.as_str();
    Preferences {
        provider: Provider::from_setting(text(PROVIDER)),
        dictation_language: Language::from_setting(text(DICTATION_LANGUAGE)),
        display_language: Language::from_setting(text(DISPLAY_LANGUAGE)),
    }
}

pub fn save(file: &Path, preferences: &Preferences) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let saved = json!({
        PROVIDER: preferences.provider.name(),
        DICTATION_LANGUAGE: preferences.dictation_language.map(Language::tag),
        DISPLAY_LANGUAGE: preferences.display_language.map(Language::tag),
    });
    std::fs::write(file, saved.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_file_named(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("buttervoice-preferences-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn what_is_saved_is_what_loads_again() {
        let file = a_file_named("round-trip.json");
        let chosen = Preferences {
            provider: Provider::Grok,
            dictation_language: Some(Language::BrazilianPortuguese),
            display_language: Some(Language::English),
        };

        save(&file, &chosen).unwrap();

        assert_eq!(load(&file), chosen);
    }

    #[test]
    fn a_file_from_before_languages_were_saved_keeps_its_provider() {
        let file = a_file_named("older.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, r#"{"provider":"grok"}"#).unwrap();

        assert_eq!(
            load(&file),
            Preferences {
                provider: Provider::Grok,
                dictation_language: None,
                display_language: None,
            }
        );
    }

    #[test]
    fn with_no_file_nothing_has_been_chosen() {
        assert_eq!(
            load(&a_file_named("never-written.json")),
            Preferences {
                provider: Provider::Local,
                dictation_language: None,
                display_language: None,
            }
        );
    }
}
