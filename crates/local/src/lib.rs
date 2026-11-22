pub mod fetch;

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::Transcript;
use transcribe_cpp::{Model, RunOptions, Session};

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum LoadError {
    #[error("the model file was not found")]
    NotFound,
    #[error("the model could not be loaded: {0}")]
    Failed(String),
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum TranscribeError {
    #[error("the audio could not be converted to 16 kHz mono")]
    Conversion,
    #[error("the speech engine failed: {0}")]
    Engine(String),
    #[error("the loaded model does not speak this language")]
    LanguageNotSpoken,
}

/// Whether a model that lists `model_languages` can transcribe `language`.
/// A model that lists none is not tied to any language.
pub fn speaks(model_languages: &[String], language: Language) -> bool {
    model_languages.is_empty()
        || model_languages
            .iter()
            .any(|code| code == engine_code(language))
}

/// The engine names languages by their two-letter code alone.
fn engine_code(language: Language) -> &'static str {
    match language {
        Language::English => "en",
        Language::BrazilianPortuguese => "pt",
    }
}

/// Holds a model loaded and ready, so only the first use pays for loading.
#[derive(Debug)]
pub struct LocalTranscriber {
    session: Mutex<Session>,
    languages: Vec<String>,
}

impl LocalTranscriber {
    pub fn load(model: &Path) -> Result<Self, LoadError> {
        let model = Model::load(model).map_err(|error| match error {
            transcribe_cpp::Error::ModelFileNotFound(_) => LoadError::NotFound,
            other => LoadError::Failed(other.to_string()),
        })?;
        let failed = |error: transcribe_cpp::Error| LoadError::Failed(error.to_string());
        let languages = model.capabilities().map_err(failed)?.languages;
        let session = model.session().map_err(failed)?;
        Ok(Self {
            session: Mutex::new(session),
            languages,
        })
    }

    pub fn speaks(&self, language: Language) -> bool {
        speaks(&self.languages, language)
    }

    pub fn transcribe(
        &self,
        audio: &CapturedAudio,
        language: Language,
    ) -> Result<Transcript, TranscribeError> {
        if !self.speaks(language) {
            return Err(TranscribeError::LanguageNotSpoken);
        }
        // A model with one language takes no hint; one with several is told
        // which to expect so it does not have to guess.
        let options = RunOptions {
            language: (self.languages.len() > 1).then(|| engine_code(language).to_string()),
            ..RunOptions::default()
        };
        let speech = audio
            .to_16khz_mono()
            .map_err(|_| TranscribeError::Conversion)?;
        let mut session = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        let heard = session
            .run(&speech.samples, &options)
            .map_err(|error| TranscribeError::Engine(error.to_string()))?;
        Ok(Transcript {
            text: heard.text.trim().to_string(),
        })
    }
}

/// The engine otherwise writes hundreds of lines to standard error for every
/// model it loads. Call once at startup, before loading a model.
pub fn silence_engine_log() {
    transcribe_cpp::disable_logging();
}
