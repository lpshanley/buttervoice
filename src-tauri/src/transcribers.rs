//! The transcriber in use. It is set up again whenever the provider or the
//! API key changes, so either takes effect without restarting the app.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use buttervoice_core::api_key::{self, KeySource};
use buttervoice_core::backend::{Backend, CATALOG, Place, best_local_fit, local_backend_for};
use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::{Provider, Transcript, TranscriptionFailure};
use buttervoice_grok::GrokTranscriber;
use buttervoice_local::fetch::{Expected, FetchError, fetch};
use buttervoice_local::{LoadError, LocalTranscriber};

use crate::preferences::{self, Preferences};
use crate::saved_key;

const API_KEY_VARIABLE: &str = "XAI_API_KEY";

#[derive(Clone)]
enum Ready {
    Starting,
    Local(Arc<LocalTranscriber>, &'static Backend),
    /// No model on this Mac speaks the person's language.
    NoLocalModel,
    LocalModelMissing,
    LocalModelUnusable,
    Grok(Arc<GrokTranscriber>, KeySource),
    GrokWithoutKey,
}

/// What a dictation was transcribed by, in which language, and how it went.
pub struct Transcribed {
    pub provider: Provider,
    /// The catalog id, such as `parakeet-ultra`.
    pub backend: &'static str,
    pub language: Language,
    pub outcome: Result<Transcript, TranscriptionFailure>,
}

/// A model on this Mac that speaks the person's language, as the settings
/// window lists it.
#[derive(serde::Serialize)]
pub struct LocalModel {
    id: &'static str,
    name: &'static str,
    bytes: u64,
    /// Benchmarked on real samples in the person's language.
    vetted: bool,
    recommended: bool,
    in_use: bool,
    downloaded: bool,
}

/// Grok's id in the catalog.
const GROK: &str = "grok";

pub struct Setup {
    pub provider: Provider,
    /// The size of the model the person's language needs, for a download.
    pub model_bytes: Option<u64>,
    /// What the settings window says about the provider in use.
    pub state: &'static str,
}

pub struct Transcribers {
    keychain_service: String,
    models_folder: PathBuf,
    preferences: Arc<preferences::Store>,
    /// Stands in for a language the person has not chosen yet.
    system_language: Language,
    ready: RwLock<Ready>,
    downloading: AtomicBool,
    cancel_download: AtomicBool,
}

impl Transcribers {
    /// Returns at once and sets up in the background: loading a model takes
    /// a moment and reading the Keychain can wait on a system prompt, and
    /// nothing else should wait with either.
    pub fn start(
        keychain_service: String,
        preferences: Arc<preferences::Store>,
        models_folder: PathBuf,
        system_language: Language,
    ) -> Arc<Self> {
        let transcribers = Arc::new(Self {
            keychain_service,
            preferences,
            system_language,
            models_folder,
            ready: RwLock::new(Ready::Starting),
            downloading: AtomicBool::new(false),
            cancel_download: AtomicBool::new(false),
        });
        std::thread::spawn({
            let transcribers = transcribers.clone();
            move || transcribers.set_up()
        });
        transcribers
    }

    pub fn transcribe(&self, audio: &CapturedAudio) -> Transcribed {
        let language = self.dictation_language();
        let local_id = || self.wanted_backend().map_or("none", |backend| backend.id);
        let (provider, backend, outcome) = match self.ready() {
            Ready::Local(local, backend) if !local.speaks(language) => (
                Provider::Local,
                backend.id,
                Err(TranscriptionFailure::LanguageNotSupported),
            ),
            Ready::Local(local, backend) => {
                let outcome = local.transcribe(audio, language).map_err(|error| {
                    tracing::error!(%error, "the local engine could not transcribe");
                    TranscriptionFailure::EngineFailed
                });
                (Provider::Local, backend.id, outcome)
            }
            // The cloud service hears quiet speech worse; the local engine
            // does not care.
            Ready::Grok(grok, _) => (
                Provider::Grok,
                GROK,
                grok.transcribe(&audio.boosted_for_transcription(), language),
            ),
            Ready::NoLocalModel => (
                Provider::Local,
                "none",
                Err(TranscriptionFailure::LanguageNotSupported),
            ),
            Ready::Starting | Ready::LocalModelMissing | Ready::LocalModelUnusable => (
                Provider::Local,
                local_id(),
                Err(TranscriptionFailure::NotConfigured),
            ),
            Ready::GrokWithoutKey => (
                Provider::Grok,
                GROK,
                Err(TranscriptionFailure::NotConfigured),
            ),
        };
        Transcribed {
            provider,
            backend,
            language,
            outcome,
        }
    }

    /// What a transcriber on this Mac makes of a recording, or `None` when
    /// there is none for the person's language. A sample is never sent to a
    /// cloud service just to suggest its text.
    pub fn heard_on_this_mac(&self, audio: &CapturedAudio) -> Option<Transcribed> {
        let language = self.dictation_language();
        match self.ready() {
            Ready::Local(local, _) if local.speaks(language) => Some(self.transcribe(audio)),
            _ => None,
        }
    }

    pub fn setup(&self) -> Setup {
        let state = match self.ready() {
            Ready::Starting => "starting",
            Ready::Local(local, _) if !local.speaks(self.dictation_language()) => {
                "language_not_supported"
            }
            Ready::NoLocalModel => "language_not_supported",
            // Recommendations need real samples in the language; until then
            // the person is told the model has not been tested with it.
            Ready::Local(_, backend)
                if !backend.vetted_for.contains(&self.dictation_language()) =>
            {
                "ready_untested"
            }
            Ready::Local(..) => "ready",
            Ready::LocalModelMissing if self.downloading.load(Ordering::Relaxed) => "downloading",
            Ready::LocalModelMissing => "model_missing",
            Ready::LocalModelUnusable => "model_unusable",
            Ready::Grok(_, KeySource::Environment) => "key_from_environment",
            Ready::Grok(_, KeySource::Saved) => "key_saved",
            Ready::GrokWithoutKey => "no_key",
        };
        Setup {
            provider: self.provider(),
            model_bytes: self
                .wanted_backend()
                .and_then(|backend| backend.model)
                .map(|model| model.bytes),
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
    /// A different language may need a different model, which is loaded
    /// before this returns.
    pub fn choose_language(&self, language: Language) -> std::io::Result<()> {
        self.change(|preferences| {
            preferences.dictation_language = Some(language);
            preferences.display_language = Some(language);
        })?;
        if self.provider() == Provider::Local && self.loaded_backend() != self.wanted_backend() {
            self.set_up();
        }
        Ok(())
    }

    /// Fetches the model the person's language needs, then loads it. Only
    /// ever run because the person asked: it is a network call.
    pub fn download_model(&self, on_progress: impl FnMut(u64)) -> Result<(), FetchError> {
        let Some(model) = self.wanted_backend().and_then(|backend| backend.model) else {
            return Ok(());
        };
        if self.downloading.swap(true, Ordering::Relaxed) {
            return Ok(());
        }
        self.cancel_download.store(false, Ordering::Relaxed);
        let expected = Expected {
            sha256: model.sha256,
            bytes: model.bytes,
        };
        let outcome = fetch(
            model.source,
            expected,
            &self.models_folder.join(model.file),
            on_progress,
            &self.cancel_download,
        );
        self.downloading.store(false, Ordering::Relaxed);
        match &outcome {
            Ok(()) => {
                tracing::info!(file = model.file, "the speech model was downloaded");
                self.set_up();
            }
            Err(error) => {
                tracing::warn!(%error, file = model.file, "the speech model was not downloaded")
            }
        }
        outcome
    }

    pub fn cancel_download(&self) {
        self.cancel_download.store(true, Ordering::Relaxed);
    }

    pub fn local_models(&self) -> Vec<LocalModel> {
        let language = self.dictation_language();
        let recommended = best_local_fit(&CATALOG, language).map(|backend| backend.id);
        let in_use = self.wanted_backend().map(|backend| backend.id);
        CATALOG
            .iter()
            .filter(|backend| backend.place == Place::ThisMac && backend.speaks.contains(&language))
            .filter_map(|backend| {
                let model = backend.model?;
                Some(LocalModel {
                    id: backend.id,
                    name: backend.name,
                    bytes: model.bytes,
                    vetted: backend.vetted_for.contains(&language),
                    recommended: recommended == Some(backend.id),
                    in_use: in_use == Some(backend.id),
                    downloaded: self.models_folder.join(model.file).exists(),
                })
            })
            .collect()
    }

    /// `None` goes back to the recommended model. A different model is
    /// loaded before this returns.
    pub fn choose_local_model(&self, id: Option<String>) -> std::io::Result<()> {
        self.change(|preferences| preferences.local_model = id)?;
        if self.provider() == Provider::Local && self.loaded_backend() != self.wanted_backend() {
            self.set_up();
        }
        Ok(())
    }

    fn wanted_backend(&self) -> Option<&'static Backend> {
        local_backend_for(
            &CATALOG,
            self.dictation_language(),
            self.preferences().local_model.as_deref(),
        )
    }

    fn loaded_backend(&self) -> Option<&'static Backend> {
        match self.ready() {
            Ready::Local(_, backend) => Some(backend),
            _ => None,
        }
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
        let Some((backend, model)) = self
            .wanted_backend()
            .and_then(|backend| Some((backend, backend.model?)))
        else {
            tracing::warn!("no model on this Mac speaks the chosen language");
            return Ready::NoLocalModel;
        };
        match LocalTranscriber::load(&self.models_folder.join(model.file)) {
            Ok(local) => {
                tracing::info!(backend = backend.id, "the local speech model is loaded");
                Ready::Local(Arc::new(local), backend)
            }
            Err(LoadError::NotFound) => {
                tracing::warn!(
                    backend = backend.id,
                    "the local speech model is not installed"
                );
                Ready::LocalModelMissing
            }
            Err(error) => {
                tracing::error!(%error, backend = backend.id, "the local speech model could not be loaded");
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
