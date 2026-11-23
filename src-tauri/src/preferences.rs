//! The choices that outlive a run, kept as JSON in the app's own folder.

use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use buttervoice_core::capture::MicrophoneChoice;
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::Provider;
use serde_json::{Value, json};

/// The language a person dictates in and the language the app is shown in
/// are saved separately, so they can differ, even while one control sets both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    pub provider: Provider,
    pub dictation_language: Option<Language>,
    pub display_language: Option<Language>,
    pub microphone: MicrophoneChoice,
    pub hotkey: HotkeyKey,
    /// The catalog id of a model chosen over the recommended one.
    pub local_model: Option<String>,
    pub check_for_updates_daily: bool,
    /// Seconds since 1970.
    pub last_update_check: Option<u64>,
    /// The label last given when recording samples, offered the next time.
    pub sample_speaker: Option<String>,
    /// Whether what is dictated is kept so it can be searched later.
    pub keep_history: bool,
    /// How fast the person types, to work out the time dictating saves.
    pub typing_words_per_minute: u32,
}

/// The saved choices, shared by every part of the app that reads them. A
/// change is written to disk first and only taken once it is saved.
pub struct Store {
    file: PathBuf,
    current: RwLock<Preferences>,
}

impl Store {
    pub fn open(file: PathBuf) -> Self {
        Self {
            current: RwLock::new(load(&file)),
            file,
        }
    }

    pub fn get(&self) -> Preferences {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn change(&self, change: impl FnOnce(&mut Preferences)) -> std::io::Result<()> {
        let mut changed = self.get();
        change(&mut changed);
        save(&self.file, &changed)?;
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = changed;
        Ok(())
    }
}

const PROVIDER: &str = "provider";
const DICTATION_LANGUAGE: &str = "dictation_language";
const DISPLAY_LANGUAGE: &str = "display_language";
const MICROPHONE: &str = "microphone";
const HOTKEY: &str = "hotkey";
const LOCAL_MODEL: &str = "local_model";
const CHECK_FOR_UPDATES_DAILY: &str = "check_for_updates_daily";
const LAST_UPDATE_CHECK: &str = "last_update_check";
const SAMPLE_SPEAKER: &str = "sample_speaker";
const KEEP_HISTORY: &str = "keep_history";
const TYPING_WORDS_PER_MINUTE: &str = "typing_words_per_minute";

/// A commonly quoted average for typing on a keyboard.
pub const AVERAGE_TYPING_WORDS_PER_MINUTE: u32 = 40;

pub fn load(file: &Path) -> Preferences {
    let saved = std::fs::read_to_string(file)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok());
    let text = |name: &str| saved.as_ref()?.get(name)?.as_str();
    let switch = |name: &str| saved.as_ref()?.get(name)?.as_bool();
    Preferences {
        provider: Provider::from_setting(text(PROVIDER)),
        dictation_language: Language::from_setting(text(DICTATION_LANGUAGE)),
        display_language: Language::from_setting(text(DISPLAY_LANGUAGE)),
        microphone: MicrophoneChoice::from_setting(text(MICROPHONE)),
        hotkey: HotkeyKey::from_setting(text(HOTKEY)),
        local_model: text(LOCAL_MODEL).map(str::to_string),
        // On unless turned off: keeping the team current matters more
        // than one request to GitHub a day.
        check_for_updates_daily: switch(CHECK_FOR_UPDATES_DAILY).unwrap_or(true),
        last_update_check: saved
            .as_ref()
            .and_then(|preferences| preferences.get(LAST_UPDATE_CHECK)?.as_u64()),
        sample_speaker: text(SAMPLE_SPEAKER).map(str::to_string),
        keep_history: switch(KEEP_HISTORY).unwrap_or(true),
        typing_words_per_minute: saved
            .as_ref()
            .and_then(|preferences| preferences.get(TYPING_WORDS_PER_MINUTE)?.as_u64())
            .and_then(|speed| u32::try_from(speed).ok())
            .filter(|speed| *speed > 0)
            .unwrap_or(AVERAGE_TYPING_WORDS_PER_MINUTE),
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
        MICROPHONE: preferences.microphone.to_setting(),
        HOTKEY: preferences.hotkey.name(),
        LOCAL_MODEL: preferences.local_model,
        CHECK_FOR_UPDATES_DAILY: preferences.check_for_updates_daily,
        LAST_UPDATE_CHECK: preferences.last_update_check,
        SAMPLE_SPEAKER: preferences.sample_speaker,
        KEEP_HISTORY: preferences.keep_history,
        TYPING_WORDS_PER_MINUTE: preferences.typing_words_per_minute,
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
            microphone: MicrophoneChoice::Device("made-up-usb-microphone".to_string()),
            hotkey: HotkeyKey::LeftControl,
            local_model: Some("parakeet-tdt-0.6b-v2".to_string()),
            check_for_updates_daily: false,
            last_update_check: Some(1_800_000_000),
            sample_speaker: Some("LPS".to_string()),
            keep_history: false,
            typing_words_per_minute: 65,
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
                microphone: MicrophoneChoice::BuiltIn,
                hotkey: HotkeyKey::RightOption,
                local_model: None,
                check_for_updates_daily: true,
                last_update_check: None,
                sample_speaker: None,
                keep_history: true,
                typing_words_per_minute: 40,
            }
        );
    }

    #[test]
    fn a_change_is_saved_and_seen_by_the_next_start() {
        let file = a_file_named("store.json");
        let store = Store::open(file.clone());

        store
            .change(|preferences| preferences.provider = Provider::Grok)
            .unwrap();

        assert_eq!(store.get().provider, Provider::Grok);
        assert_eq!(Store::open(file).get().provider, Provider::Grok);
    }

    #[test]
    fn a_change_that_cannot_be_saved_is_not_taken() {
        let blocker = a_file_named("not-a-folder");
        std::fs::create_dir_all(blocker.parent().unwrap()).unwrap();
        std::fs::write(&blocker, "a file where a folder is needed").unwrap();
        let store = Store::open(blocker.join("preferences.json"));

        let outcome = store.change(|preferences| preferences.provider = Provider::Grok);

        assert!(outcome.is_err());
        assert_eq!(store.get().provider, Provider::Local);
    }

    #[test]
    fn with_no_file_nothing_has_been_chosen() {
        assert_eq!(
            load(&a_file_named("never-written.json")),
            Preferences {
                provider: Provider::Local,
                dictation_language: None,
                display_language: None,
                microphone: MicrophoneChoice::BuiltIn,
                hotkey: HotkeyKey::RightOption,
                local_model: None,
                check_for_updates_daily: true,
                last_update_check: None,
                sample_speaker: None,
                keep_history: true,
                typing_words_per_minute: 40,
            }
        );
    }
}
