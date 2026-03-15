use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use tauri::AppHandle;

use crate::models;
use crate::remote_speech::{build_remote_config, RemoteSpeechBackend, SpeechModelEntry};
use crate::settings::{ComputeMode, Settings, SpeechProvider};
use crate::whisper_backend::{
    BackendStatus, TranscribeRequest, TranscribeResponse, WhisperBackend,
};

pub struct SpeechService {
    whisper_bin: PathBuf,
    backend_manifest_path: PathBuf,
    model_cache_dir: PathBuf,
    local_backend: Mutex<Option<Arc<WhisperBackend>>>,
    remote_backend: RemoteSpeechBackend,
}

impl SpeechService {
    pub fn new(
        whisper_bin: PathBuf,
        backend_manifest_path: PathBuf,
        model_cache_dir: PathBuf,
    ) -> Result<Self> {
        std::fs::create_dir_all(&model_cache_dir).with_context(|| {
            format!(
                "failed creating model cache at {}",
                model_cache_dir.display()
            )
        })?;

        Ok(Self {
            whisper_bin,
            backend_manifest_path,
            model_cache_dir,
            local_backend: Mutex::new(None),
            remote_backend: RemoteSpeechBackend,
        })
    }

    pub fn transcribe(
        &self,
        settings: &Settings,
        request: &TranscribeRequest,
    ) -> Result<TranscribeResponse> {
        match settings.speech_provider {
            SpeechProvider::LocalWhispercpp => self.ensure_local_backend()?.transcribe(request),
            SpeechProvider::RemoteOpenaiCompatible => {
                let config = build_remote_config(
                    settings.speech_provider,
                    settings.speech_remote_preset,
                    &settings.speech_remote_base_url,
                    &settings.speech_remote_model,
                    &settings.speech_remote_api_key,
                )?;
                self.remote_backend.transcribe(&config, request)
            }
        }
    }

    pub fn backend_status(&self, settings: &Settings) -> BackendStatus {
        match settings.speech_provider {
            SpeechProvider::LocalWhispercpp => match self.ensure_local_backend() {
                Ok(local) => {
                    let mut status = local.backend_status(settings.compute_mode);
                    status.ok = status.ok && status.provider_ok;
                    status.backend = "local/whispercpp".to_string();
                    status.active_provider = settings.speech_provider.as_str().to_string();
                    status.provider_label = "local/whispercpp".to_string();
                    status.provider_ok = true;
                    status.provider_error = None;
                    status.remote_base_url = None;
                    status.remote_model = None;
                    status
                }
                Err(err) => BackendStatus {
                    ok: false,
                    backend: "local/whispercpp".to_string(),
                    active_provider: settings.speech_provider.as_str().to_string(),
                    provider_label: "local/whispercpp".to_string(),
                    provider_ok: false,
                    provider_error: Some(err.to_string()),
                    remote_base_url: None,
                    remote_model: None,
                    binary_available: self.whisper_bin.exists(),
                    binary_path: Some(self.whisper_bin.display().to_string()),
                    selected_compute_mode: settings.compute_mode.as_str().to_string(),
                    effective_compute_mode: None,
                    last_fallback_reason: None,
                },
            },
            SpeechProvider::RemoteOpenaiCompatible => {
                let config = build_remote_config(
                    settings.speech_provider,
                    settings.speech_remote_preset,
                    &settings.speech_remote_base_url,
                    &settings.speech_remote_model,
                    &settings.speech_remote_api_key,
                );
                match config {
                    Ok(config) => {
                        let mut status = self.remote_backend.backend_status(&config);
                        status.binary_available = self.whisper_bin.exists();
                        status.binary_path = Some(self.whisper_bin.display().to_string());
                        status.selected_compute_mode = settings.compute_mode.as_str().to_string();
                        status
                    }
                    Err(err) => BackendStatus {
                        ok: false,
                        backend: format!("remote/{}", settings.speech_remote_preset.as_str()),
                        active_provider: settings.speech_provider.as_str().to_string(),
                        provider_label: format!(
                            "remote/{}",
                            settings.speech_remote_preset.as_str()
                        ),
                        provider_ok: false,
                        provider_error: Some(err.to_string()),
                        remote_base_url: Some(settings.speech_remote_base_url.clone()),
                        remote_model: Some(settings.speech_remote_model.clone()),
                        binary_available: self.whisper_bin.exists(),
                        binary_path: Some(self.whisper_bin.display().to_string()),
                        selected_compute_mode: settings.compute_mode.as_str().to_string(),
                        effective_compute_mode: None,
                        last_fallback_reason: None,
                    },
                }
            }
        }
    }

    pub fn download_model(&self, model_id: &str) -> Result<PathBuf> {
        self.ensure_local_backend()?.download_model(model_id)
    }

    pub fn download_model_with_progress(
        &self,
        model_id: &str,
        app_handle: &AppHandle,
        cancel_flag: &Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<PathBuf> {
        self.ensure_local_backend()?
            .download_model_with_progress(model_id, app_handle, cancel_flag)
    }

    pub fn list_downloaded_models(&self) -> Result<Vec<String>> {
        let mut downloaded = Vec::new();
        let model_dir = self.model_cache_dir.join("whisper.cpp");
        std::fs::create_dir_all(&model_dir)
            .with_context(|| format!("failed creating model directory {}", model_dir.display()))?;

        for model in models::available_models() {
            let spec = models::download_spec(&model.id)
                .with_context(|| format!("missing download spec for {}", model.id))?;
            let destination = model_dir.join(spec.file_name);
            if destination.exists() && destination.metadata()?.len() > 0 {
                downloaded.push(model.id);
            }
        }

        downloaded.sort();
        Ok(downloaded)
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        self.ensure_local_backend()?.delete_model(model_id)
    }

    pub fn warm_up(&self, model_id: &str, compute_mode: ComputeMode) -> Result<()> {
        self.ensure_local_backend()?.warm_up(model_id, compute_mode)
    }

    pub fn list_remote_models(&self, settings: &Settings) -> Result<Vec<SpeechModelEntry>> {
        let config = build_remote_config(
            settings.speech_provider,
            settings.speech_remote_preset,
            &settings.speech_remote_base_url,
            &settings.speech_remote_model,
            &settings.speech_remote_api_key,
        )?;
        self.remote_backend.list_models(&config)
    }

    pub fn test_remote_connection(&self, settings: &Settings) -> Result<String> {
        let config = build_remote_config(
            settings.speech_provider,
            settings.speech_remote_preset,
            &settings.speech_remote_base_url,
            &settings.speech_remote_model,
            &settings.speech_remote_api_key,
        )?;
        self.remote_backend.test_connection(&config)
    }

    fn ensure_local_backend(&self) -> Result<Arc<WhisperBackend>> {
        if let Some(existing) = self.local_backend.lock().clone() {
            return Ok(existing);
        }

        let backend = Arc::new(WhisperBackend::new(
            self.whisper_bin.clone(),
            self.backend_manifest_path.clone(),
            self.model_cache_dir.clone(),
        )?);

        let mut slot = self.local_backend.lock();
        if let Some(existing) = slot.clone() {
            Ok(existing)
        } else {
            *slot = Some(backend.clone());
            Ok(backend)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SpeechService;
    use crate::settings::{Settings, SpeechProvider};
    use std::path::PathBuf;

    #[test]
    fn remote_backend_status_does_not_require_local_bundle() {
        let service = SpeechService::new(
            PathBuf::from("/tmp/buttervoice-missing-whisper-cli"),
            PathBuf::from("/tmp/buttervoice-missing-manifest.json"),
            std::env::temp_dir().join("buttervoice-speech-service-test"),
        )
        .unwrap();

        let settings = Settings {
            speech_provider: SpeechProvider::RemoteOpenaiCompatible,
            speech_remote_base_url: "http://127.0.0.1:8000/v1".to_string(),
            speech_remote_model: "Systran/faster-whisper-small.en".to_string(),
            ..Settings::default()
        };

        let status = service.backend_status(&settings);
        assert!(status.provider_ok);
        assert_eq!(status.active_provider, "remote_openai_compatible");
        assert_eq!(status.provider_label, "remote/speaches");
    }
}
