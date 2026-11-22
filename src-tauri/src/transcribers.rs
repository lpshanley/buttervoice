//! The transcription client in use, rebuilt whenever the API key changes so
//! a newly saved key takes effect without restarting the app.

use std::sync::{Arc, PoisonError, RwLock};

use buttervoice_core::api_key::{self, KeySource};
use buttervoice_grok::GrokTranscriber;

use crate::saved_key;

const API_KEY_VARIABLE: &str = "XAI_API_KEY";

pub struct Transcribers {
    keychain_service: String,
    current: RwLock<Option<(Arc<GrokTranscriber>, KeySource)>>,
}

impl Transcribers {
    pub fn new(keychain_service: String) -> Self {
        let transcribers = Self {
            keychain_service,
            current: RwLock::new(None),
        };
        transcribers.reload();
        transcribers
    }

    pub fn current(&self) -> Option<Arc<GrokTranscriber>> {
        self.read()
            .as_ref()
            .map(|(transcriber, _)| transcriber.clone())
    }

    pub fn key_source(&self) -> Option<KeySource> {
        self.read().as_ref().map(|(_, source)| *source)
    }

    pub fn save_key(&self, key: &str) -> Result<(), String> {
        if key.trim().is_empty() {
            return Err("Enter a key first.".to_string());
        }
        saved_key::save(&self.keychain_service, key.trim())?;
        self.reload();
        Ok(())
    }

    pub fn forget_key(&self) -> Result<(), String> {
        saved_key::forget(&self.keychain_service)?;
        self.reload();
        Ok(())
    }

    fn reload(&self) {
        let chosen = api_key::choose(std::env::var(API_KEY_VARIABLE).ok(), || {
            saved_key::load(&self.keychain_service)
        });
        let current = chosen.and_then(|chosen| {
            match GrokTranscriber::new(buttervoice_grok::DEFAULT_BASE_URL, chosen.key) {
                Ok(transcriber) => Some((Arc::new(transcriber), chosen.source)),
                Err(error) => {
                    tracing::error!(%error, "the transcription client could not be set up");
                    None
                }
            }
        });
        if current.is_none() {
            tracing::warn!("no xAI API key is available; dictations will not be transcribed");
        }
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = current;
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Option<(Arc<GrokTranscriber>, KeySource)>> {
        self.current.read().unwrap_or_else(PoisonError::into_inner)
    }
}
