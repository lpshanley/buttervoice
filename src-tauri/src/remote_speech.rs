use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::{multipart, Client};
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};

use crate::settings::{SpeechProvider, SpeechRemotePreset};
use crate::whisper_backend::{BackendStatus, TranscribeRequest, TranscribeResponse};

const REMOTE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechModelEntry {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RemoteSpeechConfig {
    pub provider: SpeechProvider,
    pub preset: SpeechRemotePreset,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Default)]
pub struct RemoteSpeechBackend;

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelResponseEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelResponseEntry {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

impl RemoteSpeechBackend {
    pub fn transcribe(
        &self,
        config: &RemoteSpeechConfig,
        request: &TranscribeRequest,
    ) -> Result<TranscribeResponse> {
        validate_remote_config(config)?;

        if !request.audio_path.exists() {
            return Ok(json_error(
                "AUDIO_DECODE_ERROR",
                &format!(
                    "audio file does not exist: {}",
                    request.audio_path.display()
                ),
                &request.request_id,
            ));
        }

        let endpoint = format!(
            "{}/audio/transcriptions",
            config.base_url.trim_end_matches('/')
        );
        let client = Client::builder()
            .timeout(REMOTE_TIMEOUT)
            .build()
            .context("failed creating remote transcription client")?;

        let file_name = request
            .audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio.wav");
        let audio_bytes = std::fs::read(&request.audio_path).with_context(|| {
            format!("failed reading audio file {}", request.audio_path.display())
        })?;

        let mut form = multipart::Form::new()
            .text("model", config.model.clone())
            .part(
                "file",
                multipart::Part::bytes(audio_bytes).file_name(file_name.to_string()),
            );

        if !request.language.is_empty() && request.language != "auto" {
            form = form.text("language", request.language.clone());
        }
        if !request.prompt.trim().is_empty() {
            form = form.text("prompt", request.prompt.clone());
        }

        let start = std::time::Instant::now();
        let mut req = client
            .post(&endpoint)
            .header(USER_AGENT, "ButterVoice/1.0")
            .multipart(form);
        if !config.api_key.trim().is_empty() {
            req = req.bearer_auth(config.api_key.trim());
        }

        let response = req.send().with_context(|| {
            format!("failed sending remote transcription request to {endpoint}")
        })?;
        let duration_ms = start.elapsed().as_millis() as u64;
        let status = response.status();
        let body = response.text().with_context(|| {
            format!("failed reading remote transcription response from {endpoint}")
        })?;

        if !status.is_success() {
            let detail = extract_error_message(&body);
            let code = match status.as_u16() {
                401 | 403 => "REMOTE_AUTH_FAILED",
                404 => "REMOTE_MODEL_NOT_FOUND",
                408 | 429 => "REMOTE_PROVIDER_UNAVAILABLE",
                500..=599 => "REMOTE_PROVIDER_UNAVAILABLE",
                _ => "REMOTE_BAD_RESPONSE",
            };
            return Ok(json_error(
                code,
                &format!(
                    "remote {} transcription failed with status {}: {}",
                    config.preset.as_str(),
                    status,
                    detail
                ),
                &request.request_id,
            ));
        }

        let parsed: TranscriptionResponse = serde_json::from_str(&body).with_context(|| {
            format!(
                "failed decoding remote transcription response from {}: {}",
                endpoint, body
            )
        })?;

        Ok(TranscribeResponse {
            request_id: request.request_id.clone(),
            text: parsed.text.trim().to_string(),
            duration_ms,
            backend: Some(format!("remote/{}", config.preset.as_str())),
            effective_compute_mode: None,
            fallback_reason: None,
            error_code: None,
            error_message: None,
        })
    }

    pub fn list_models(&self, config: &RemoteSpeechConfig) -> Result<Vec<SpeechModelEntry>> {
        validate_remote_base_url(config)?;

        let endpoint = format!("{}/models", config.base_url.trim_end_matches('/'));
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed creating remote speech client")?;

        let mut req = client.get(&endpoint).header(USER_AGENT, "ButterVoice/1.0");
        if !config.api_key.trim().is_empty() {
            req = req.bearer_auth(config.api_key.trim());
        }

        let response = req
            .send()
            .with_context(|| format!("failed requesting remote models from {endpoint}"))?;
        let status = response.status();
        let body = response
            .text()
            .with_context(|| format!("failed reading remote model response from {endpoint}"))?;

        if !status.is_success() {
            bail!(
                "remote {} model list failed with status {}: {}",
                config.preset.as_str(),
                status,
                extract_error_message(&body)
            );
        }

        let parsed: ModelsResponse = serde_json::from_str(&body)
            .with_context(|| format!("failed decoding remote model list: {body}"))?;
        let mut models: Vec<_> = parsed
            .data
            .into_iter()
            .map(|entry| SpeechModelEntry {
                id: entry.id.clone(),
                name: entry.name.or(Some(entry.id)),
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    pub fn test_connection(&self, config: &RemoteSpeechConfig) -> Result<String> {
        validate_remote_base_url(config)?;
        let models = self.list_models(config)?;
        let preview = models
            .first()
            .map(|entry| entry.id.as_str())
            .unwrap_or("no models returned");
        Ok(format!(
            "Connected to {}. Example model: {}",
            config.preset.as_str(),
            preview
        ))
    }

    pub fn backend_status(&self, config: &RemoteSpeechConfig) -> BackendStatus {
        let base_ok = !config.base_url.trim().is_empty();
        let model_ok = !config.model.trim().is_empty();
        let provider_ok = base_ok && model_ok;
        let provider_error = if provider_ok {
            None
        } else if !base_ok {
            Some("Remote base URL is not configured.".to_string())
        } else {
            Some("Remote speech model is not configured.".to_string())
        };

        BackendStatus {
            ok: provider_ok,
            backend: format!("remote/{}", config.preset.as_str()),
            active_provider: config.provider.as_str().to_string(),
            provider_label: format!("remote/{}", config.preset.as_str()),
            provider_ok,
            provider_error,
            remote_base_url: Some(config.base_url.clone()),
            remote_model: Some(config.model.clone()),
            binary_available: false,
            binary_path: None,
            selected_compute_mode: "n/a".to_string(),
            effective_compute_mode: None,
            last_fallback_reason: None,
        }
    }
}

fn validate_remote_base_url(config: &RemoteSpeechConfig) -> Result<()> {
    if config.base_url.trim().is_empty() {
        bail!("remote speech base URL is empty");
    }
    Ok(())
}

fn validate_remote_config(config: &RemoteSpeechConfig) -> Result<()> {
    validate_remote_base_url(config)?;
    if config.model.trim().is_empty() {
        bail!("remote speech model is empty");
    }
    Ok(())
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<ErrorEnvelope>(body)
        .ok()
        .and_then(|payload| payload.error.and_then(|err| err.message))
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| body.trim().to_string())
}

fn json_error(error_code: &str, error_message: &str, request_id: &str) -> TranscribeResponse {
    TranscribeResponse {
        request_id: request_id.to_string(),
        text: String::new(),
        duration_ms: 0,
        backend: None,
        effective_compute_mode: None,
        fallback_reason: None,
        error_code: Some(error_code.to_string()),
        error_message: Some(error_message.to_string()),
    }
}

pub fn build_remote_config(
    provider: SpeechProvider,
    preset: SpeechRemotePreset,
    base_url: &str,
    model: &str,
    api_key: &str,
) -> Result<RemoteSpeechConfig> {
    if !matches!(provider, SpeechProvider::RemoteOpenaiCompatible) {
        return Err(anyhow!("remote speech config requested for local provider"));
    }

    Ok(RemoteSpeechConfig {
        provider,
        preset,
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        model: model.trim().to_string(),
        api_key: api_key.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{build_remote_config, RemoteSpeechBackend, RemoteSpeechConfig};
    use crate::settings::{ComputeMode, SpeechProvider, SpeechRemotePreset};
    use crate::whisper_backend::TranscribeRequest;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn spawn_server(response_body: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();

            let mut bytes = Vec::new();
            let mut buf = [0u8; 4096];
            let mut header_len = None;
            let mut content_len = 0usize;

            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(read) => {
                        bytes.extend_from_slice(&buf[..read]);
                        if header_len.is_none() {
                            if let Some(idx) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                                let header_end = idx + 4;
                                header_len = Some(header_end);
                                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                                for line in headers.lines() {
                                    if let Some(value) = line.strip_prefix("Content-Length: ") {
                                        content_len = value.trim().parse::<usize>().unwrap_or(0);
                                    }
                                }
                            }
                        }

                        if let Some(header_end) = header_len {
                            if bytes.len() >= header_end + content_len {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            tx.send(String::from_utf8_lossy(&bytes).to_string())
                .unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        (format!("http://{addr}"), rx)
    }

    fn temp_audio_file() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let path = std::env::temp_dir().join(format!("buttervoice-remote-test-{ts}.wav"));
        fs::write(&path, b"RIFFremote").unwrap();
        path
    }

    fn request_for(path: PathBuf) -> TranscribeRequest {
        TranscribeRequest {
            request_id: "req-1".to_string(),
            audio_path: path,
            model_id: "Systran/faster-whisper-small.en".to_string(),
            language: "en".to_string(),
            compute_mode: ComputeMode::Auto,
            beam_size: 2,
            prompt: "Hello ButterVoice".to_string(),
            no_speech_thold: 0.6,
            temperature: 0.0,
            temperature_inc: 0.2,
            threads: 0,
        }
    }

    fn config(base_url: String) -> RemoteSpeechConfig {
        build_remote_config(
            SpeechProvider::RemoteOpenaiCompatible,
            SpeechRemotePreset::Speaches,
            &base_url,
            "Systran/faster-whisper-small.en",
            "secret-token",
        )
        .unwrap()
    }

    #[test]
    fn build_remote_config_rejects_local_provider() {
        let err = build_remote_config(
            SpeechProvider::LocalWhispercpp,
            SpeechRemotePreset::Speaches,
            "http://127.0.0.1:8000/v1",
            "model",
            "",
        )
        .unwrap_err();
        assert!(err.to_string().contains("local provider"));
    }

    #[test]
    fn list_models_parses_openai_style_payload() {
        let (base_url, rx) =
            spawn_server(r#"{"data":[{"id":"model-a","name":"Model A"},{"id":"model-b"}]}"#);
        let backend = RemoteSpeechBackend;
        let models = backend.list_models(&config(base_url)).unwrap();
        let raw_request = rx.recv().unwrap();

        assert!(raw_request.starts_with("GET /models HTTP/1.1"));
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "model-a");
        assert_eq!(models[0].name.as_deref(), Some("Model A"));
    }

    #[test]
    fn transcribe_posts_multipart_form() {
        let (base_url, rx) = spawn_server(r#"{"text":"hello remote world"}"#);
        let backend = RemoteSpeechBackend;
        let audio = temp_audio_file();
        let response = backend
            .transcribe(&config(base_url), &request_for(audio.clone()))
            .unwrap();
        let raw_request = rx.recv().unwrap();

        assert!(raw_request.starts_with("POST /audio/transcriptions HTTP/1.1"));
        assert!(raw_request
            .to_ascii_lowercase()
            .contains("authorization: bearer secret-token"));
        assert!(raw_request.contains("name=\"model\""));
        assert!(raw_request.contains("Systran/faster-whisper-small.en"));
        assert!(raw_request.contains("name=\"language\""));
        assert!(raw_request.contains("name=\"prompt\""));
        assert_eq!(response.text, "hello remote world");

        let _ = fs::remove_file(audio);
    }
}
