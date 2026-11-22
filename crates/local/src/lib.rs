use std::path::Path;
use std::sync::{Mutex, PoisonError};

use buttervoice_core::capture::CapturedAudio;
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
}

/// Holds a model loaded and ready, so only the first use pays for loading.
#[derive(Debug)]
pub struct LocalTranscriber {
    session: Mutex<Session>,
}

impl LocalTranscriber {
    pub fn load(model: &Path) -> Result<Self, LoadError> {
        let model = Model::load(model).map_err(|error| match error {
            transcribe_cpp::Error::ModelFileNotFound(_) => LoadError::NotFound,
            other => LoadError::Failed(other.to_string()),
        })?;
        let session = model
            .session()
            .map_err(|error| LoadError::Failed(error.to_string()))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }

    pub fn transcribe(&self, audio: &CapturedAudio) -> Result<Transcript, TranscribeError> {
        let speech = audio
            .to_16khz_mono()
            .map_err(|_| TranscribeError::Conversion)?;
        let mut session = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        let heard = session
            .run(&speech.samples, &RunOptions::default())
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
