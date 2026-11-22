//! The transcriber in use. It is set up again whenever the provider or the
//! API key changes, so either takes effect without restarting the app.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use buttervoice_core::api_key::{self, KeySource};
use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::{Provider, Transcript, TranscriptionFailure};
use buttervoice_grok::GrokTranscriber;
use buttervoice_local::{LoadError, LocalTranscriber};

use crate::preferences::{self, Preferences};
use crate::saved_key;

const API_KEY_VARIABLE: &str = "XAI_API_KEY";

#[derive(Clone)]
enum Ready {
    Starting,
    Local(Arc<LocalTranscriber>),
    LocalModelMissing,
    LocalModelUnusable,
    Grok(Arc<GrokTranscriber>, KeySource),
    GrokWithoutKey,
}

pub struct Setup {
    pub provider: Provider,
    /// What the settings window says about the provider in use.
    pub state: &'static str,
}

pub struct Transcribers {
    keychain_service: String,
    model_file: PathBuf,
    preferences: Arc<preferences::Store>,
    /// Stands in for a language the person has not chosen yet.
    system_language: Language,
    ready: RwLock<Ready>,
}

impl Transcribers {
    /// Returns at once and sets up in the background: loading a model takes
    /// a moment and reading the Keychain can wait on a system prompt, and
    /// nothing else should wait with either.
    pub fn start(
        keychain_service: String,
        preferences: Arc<preferences::Store>,
        model_file: PathBuf,
        system_language: Language,
    ) -> Arc<Self> {
        let transcribers = Arc::new(Self {
            keychain_service,
            preferences,
            system_language,
            model_file,
            ready: RwLock::new(Ready::Starting),
        });
        std::thread::spawn({
            let transcribers = transcribers.clone();
            move || transcribers.set_up()
        });
        transcribers
    }

    pub fn transcribe(
        &self,
        audio: &CapturedAudio,
    ) -> (Provider, Language, Result<Transcript, TranscriptionFailure>) {
        let language = self.dictation_language();
        let (provider, outcome) = match self.ready() {
            Ready::Local(local) if !local.speaks(language) => (
                Provider::Local,
                Err(TranscriptionFailure::LanguageNotSupported),
            ),
            Ready::Local(local) => {
                let outcome = local.transcribe(audio, language).map_err(|error| {
                    tracing::error!(%error, "the local engine could not transcribe");
                    TranscriptionFailure::EngineFailed
                });
                (Provider::Local, outcome)
            }
            // The cloud service hears quiet speech worse; the local engine
            // does not care.
            Ready::Grok(grok, _) => (
                Provider::Grok,
                grok.transcribe(&audio.boosted_for_transcription(), language),
            ),
            Ready::Starting | Ready::LocalModelMissing | Ready::LocalModelUnusable => {
                (Provider::Local, Err(TranscriptionFailure::NotConfigured))
            }
            Ready::GrokWithoutKey => (Provider::Grok, Err(TranscriptionFailure::NotConfigured)),
        };
        (provider, language, outcome)
    }

    /// What a transcriber on this Mac makes of a recording, or `None` when
    /// there is none for the person's language. A sample is never sent to a
    /// cloud service just to suggest its text.
    pub fn heard_on_this_mac(
        &self,
        audio: &CapturedAudio,
    ) -> Option<(Provider, Language, Result<Transcript, TranscriptionFailure>)> {
        let language = self.dictation_language();
        match self.ready() {
            Ready::Local(local) if local.speaks(language) => Some(self.transcribe(audio)),
            _ => None,
        }
    }

    pub fn setup(&self) -> Setup {
        let state = match self.ready() {
            Ready::Starting => "starting",
            Ready::Local(local) if !local.speaks(self.dictation_language()) => {
                "language_not_supported"
            }
            Ready::Local(_) => "ready",
            Ready::LocalModelMissing => "model_missing",
            Ready::LocalModelUnusable => "model_unusable",
            Ready::Grok(_, KeySource::Environment) => "key_from_environment",
            Ready::Grok(_, KeySource::Saved) => "key_saved",
            Ready::GrokWithoutKey => "no_key",
        };
        Setup {
            provider: self.provider(),
            state,
        }
    }

    pub fn choose_provider(&self, provider: Provider) -> std::io::Result<()> {
        self.change(|preferences| preferences.provider = provider)?;
        self.set_up();
        Ok(())
    }

    /// One choice sets both the language a person dictates in and the one
    /// the app is shown in. They are saved apart so they can differ later.
    pub fn choose_language(&self, language: Language) -> std::io::Result<()> {
        self.change(|preferences| {
            preferences.dictation_language = Some(language);
            preferences.display_language = Some(language);
        })
    }

    pub fn language_chosen(&self) -> bool {
        self.preferences().dictation_language.is_some()
    }

    pub fn dictation_language(&self) -> Language {
        self.preferences()
            .dictation_language
            .unwrap_or(self.system_language)
    }

    pub fn display_language(&self) -> Language {
        self.preferences()
            .display_language
            .unwrap_or(self.system_language)
    }

    fn change(&self, change: impl FnOnce(&mut Preferences)) -> std::io::Result<()> {
        self.preferences.change(change)
    }

    pub fn save_key(&self, key: &str) -> Result<(), String> {
        saved_key::save(&self.keychain_service, key.trim())?;
        self.set_up();
        Ok(())
    }

    pub fn forget_key(&self) -> Result<(), String> {
        saved_key::forget(&self.keychain_service)?;
        self.set_up();
        Ok(())
    }

    fn set_up(&self) {
        let ready = self
            .provider()
            .set_up(|| self.local_engine(), || self.grok_client());
        *self.ready.write().unwrap_or_else(PoisonError::into_inner) = ready;
    }

    fn local_engine(&self) -> Ready {
        match LocalTranscriber::load(&self.model_file) {
            Ok(local) => {
                tracing::info!("the local speech model is loaded");
                Ready::Local(Arc::new(local))
            }
            Err(LoadError::NotFound) => {
                tracing::warn!("the local speech model is not installed");
                Ready::LocalModelMissing
            }
            Err(error) => {
                tracing::error!(%error, "the local speech model could not be loaded");
                Ready::LocalModelUnusable
            }
        }
    }

    fn grok_client(&self) -> Ready {
        let chosen = api_key::choose(std::env::var(API_KEY_VARIABLE).ok(), || {
            saved_key::load(&self.keychain_service)
        });
        let Some(chosen) = chosen else {
            tracing::warn!("no xAI API key is available; dictations will not be transcribed");
            return Ready::GrokWithoutKey;
        };
        match GrokTranscriber::new(buttervoice_grok::DEFAULT_BASE_URL, chosen.key) {
            Ok(grok) => Ready::Grok(Arc::new(grok), chosen.source),
            Err(error) => {
                tracing::error!(%error, "the transcription client could not be set up");
                Ready::GrokWithoutKey
            }
        }
    }

    fn provider(&self) -> Provider {
        self.preferences().provider
    }

    fn preferences(&self) -> Preferences {
        self.preferences.get()
    }

    fn ready(&self) -> Ready {
        self.ready
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
