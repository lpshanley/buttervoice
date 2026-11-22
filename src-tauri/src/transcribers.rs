//! The transcriber in use. It is set up again whenever the provider or the
//! API key changes, so either takes effect without restarting the app.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use buttervoice_core::api_key::{self, KeySource};
use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::transcription::{Provider, Transcript, TranscriptionFailure};
use buttervoice_grok::GrokTranscriber;
use buttervoice_local::{LoadError, LocalTranscriber};

use crate::{preferences, saved_key};

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
    preferences_file: PathBuf,
    model_file: PathBuf,
    provider: RwLock<Provider>,
    ready: RwLock<Ready>,
}

impl Transcribers {
    /// Returns at once and sets up in the background: loading a model takes
    /// a moment and reading the Keychain can wait on a system prompt, and
    /// nothing else should wait with either.
    pub fn start(
        keychain_service: String,
        preferences_file: PathBuf,
        model_file: PathBuf,
    ) -> Arc<Self> {
        let transcribers = Arc::new(Self {
            keychain_service,
            provider: RwLock::new(preferences::load_provider(&preferences_file)),
            preferences_file,
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
    ) -> (Provider, Result<Transcript, TranscriptionFailure>) {
        match self.ready() {
            Ready::Local(local) => {
                let outcome = local.transcribe(audio).map_err(|error| {
                    tracing::error!(%error, "the local engine could not transcribe");
                    TranscriptionFailure::EngineFailed
                });
                (Provider::Local, outcome)
            }
            // The cloud service hears quiet speech worse; the local engine
            // does not care.
            Ready::Grok(grok, _) => (
                Provider::Grok,
                grok.transcribe(&audio.boosted_for_transcription()),
            ),
            Ready::Starting | Ready::LocalModelMissing | Ready::LocalModelUnusable => {
                (Provider::Local, Err(TranscriptionFailure::NotConfigured))
            }
            Ready::GrokWithoutKey => (Provider::Grok, Err(TranscriptionFailure::NotConfigured)),
        }
    }

    pub fn setup(&self) -> Setup {
        let state = match self.ready() {
            Ready::Starting => "starting",
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
        preferences::save_provider(&self.preferences_file, provider)?;
        *self
            .provider
            .write()
            .unwrap_or_else(PoisonError::into_inner) = provider;
        self.set_up();
        Ok(())
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
        *self.provider.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn ready(&self) -> Ready {
        self.ready
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
