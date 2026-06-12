use std::io::Cursor;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::{multipart, Client};
use reqwest::header::USER_AGENT;
use serde::Deserialize;

use crate::whisper_backend::{
    BackendStatus, TokenConfidence, TranscribeRequest, TranscribeResponse,
};

const GROK_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_GROK_BASE_URL: &str = "https://api.x.ai/v1";
/// Grok's STT endpoint accepts at most 100 keyterms of 50 chars each.
const MAX_KEYTERMS: usize = 100;
const MAX_KEYTERM_CHARS: usize = 50;

#[derive(Debug, Clone)]
pub struct GrokSpeechConfig {
    pub base_url: String,
    pub api_key: String,
    pub text_formatting: bool,
    pub filler_words: bool,
    pub keyterms: Vec<String>,
}

#[derive(Debug, Default)]
pub struct GrokSpeechBackend;

#[derive(Debug, Deserialize)]
struct SttResponse {
    text: String,
    #[serde(default)]
    words: Vec<SttWord>,
}

#[derive(Debug, Deserialize)]
struct SttWord {
    text: String,
    #[serde(default)]
    confidence: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

impl GrokSpeechBackend {
    pub fn transcribe(
        &self,
        config: &GrokSpeechConfig,
        request: &TranscribeRequest,
    ) -> Result<TranscribeResponse> {
        validate_grok_config(config)?;

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

        let file_name = request
            .audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio.wav");
        let audio_bytes = std::fs::read(&request.audio_path).with_context(|| {
            format!("failed reading audio file {}", request.audio_path.display())
        })?;

        let form = build_form(
            config,
            &request.language,
            multipart::Part::bytes(audio_bytes).file_name(file_name.to_string()),
        );

        let endpoint = stt_endpoint(&config.base_url);
        let client = Client::builder()
            .timeout(GROK_TIMEOUT)
            .build()
            .context("failed creating Grok transcription client")?;

        let start = std::time::Instant::now();
        let response = client
            .post(&endpoint)
            .header(USER_AGENT, "ButterVoice/1.0")
            .bearer_auth(config.api_key.trim())
            .multipart(form)
            .send()
            .with_context(|| format!("failed sending Grok transcription request to {endpoint}"))?;
        let duration_ms = start.elapsed().as_millis() as u64;
        let status = response.status();
        let body = response.text().with_context(|| {
            format!("failed reading Grok transcription response from {endpoint}")
        })?;

        if !status.is_success() {
            let detail = extract_error_message(&body);
            let (code, message) = match status.as_u16() {
                401 | 403 => (
                    "REMOTE_AUTH_FAILED",
                    format!("Grok rejected the API key (status {status}): {detail}"),
                ),
                413 => (
                    "REMOTE_BAD_RESPONSE",
                    format!(
                        "recording too large for Grok transcription (status {status}): {detail}"
                    ),
                ),
                408 | 429 => (
                    "REMOTE_PROVIDER_UNAVAILABLE",
                    format!(
                        "Grok transcription throttled or timed out (status {status}): {detail}"
                    ),
                ),
                500..=599 => (
                    "REMOTE_PROVIDER_UNAVAILABLE",
                    format!("Grok transcription failed with status {status}: {detail}"),
                ),
                _ => (
                    "REMOTE_BAD_RESPONSE",
                    format!("Grok transcription failed with status {status}: {detail}"),
                ),
            };
            return Ok(json_error(code, &message, &request.request_id));
        }

        let parsed: SttResponse = serde_json::from_str(&body).with_context(|| {
            format!("failed decoding Grok transcription response from {endpoint}: {body}")
        })?;

        let token_confidences: Vec<TokenConfidence> = parsed
            .words
            .into_iter()
            .filter_map(|word| {
                word.confidence.map(|prob| TokenConfidence {
                    text: word.text,
                    prob,
                })
            })
            .collect();

        Ok(TranscribeResponse {
            request_id: request.request_id.clone(),
            text: parsed.text.trim().to_string(),
            duration_ms,
            backend: Some("remote/grok".to_string()),
            effective_compute_mode: None,
            fallback_reason: None,
            error_code: None,
            error_message: None,
            token_confidences: if token_confidences.is_empty() {
                None
            } else {
                Some(token_confidences)
            },
            no_speech_prob: None,
        })
    }

    /// Grok has no model-listing endpoint, so probe with a short silent WAV.
    pub fn test_connection(&self, config: &GrokSpeechConfig) -> Result<String> {
        validate_grok_config(config)?;

        let endpoint = stt_endpoint(&config.base_url);
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed creating Grok speech client")?;

        let form = multipart::Form::new().part(
            "file",
            multipart::Part::bytes(silent_wav()).file_name("probe.wav".to_string()),
        );

        let response = client
            .post(&endpoint)
            .header(USER_AGENT, "ButterVoice/1.0")
            .bearer_auth(config.api_key.trim())
            .multipart(form)
            .send()
            .with_context(|| format!("failed reaching Grok speech API at {endpoint}"))?;
        let status = response.status();
        let body = response.text().unwrap_or_default();

        match status.as_u16() {
            200..=299 => Ok("Connected to Grok speech-to-text.".to_string()),
            401 | 403 => bail!(
                "Grok rejected the API key (status {}): {}",
                status,
                extract_error_message(&body)
            ),
            500..=599 => bail!(
                "Grok speech API unavailable (status {}): {}",
                status,
                extract_error_message(&body)
            ),
            // Other 4xx (e.g. the probe audio being rejected) still proves the
            // key was accepted before request validation failed.
            _ => Ok(format!(
                "Connected to Grok speech-to-text (API reachable, probe returned status {status})."
            )),
        }
    }

    pub fn backend_status(&self, config: &GrokSpeechConfig) -> BackendStatus {
        let provider_ok = !config.api_key.trim().is_empty();
        let provider_error = if provider_ok {
            None
        } else {
            Some("Grok API key is not configured.".to_string())
        };

        BackendStatus {
            ok: provider_ok,
            backend: "remote/grok".to_string(),
            active_provider: crate::settings::SpeechProvider::RemoteGrok
                .as_str()
                .to_string(),
            provider_label: "remote/grok".to_string(),
            provider_ok,
            provider_error,
            remote_base_url: Some(config.base_url.clone()),
            remote_model: Some("grok-stt".to_string()),
            binary_available: false,
            binary_path: None,
            selected_compute_mode: "n/a".to_string(),
            effective_compute_mode: None,
            last_fallback_reason: None,
        }
    }
}

pub fn build_grok_config(
    api_key: &str,
    text_formatting: bool,
    filler_words: bool,
    keyterms: &[String],
) -> GrokSpeechConfig {
    GrokSpeechConfig {
        base_url: DEFAULT_GROK_BASE_URL.to_string(),
        api_key: api_key.trim().to_string(),
        text_formatting,
        filler_words,
        keyterms: keyterms
            .iter()
            .map(|term| term.trim())
            .filter(|term| !term.is_empty() && term.chars().count() <= MAX_KEYTERM_CHARS)
            .take(MAX_KEYTERMS)
            .map(str::to_string)
            .collect(),
    }
}

fn stt_endpoint(base_url: &str) -> String {
    format!("{}/stt", base_url.trim_end_matches('/'))
}

fn build_form(
    config: &GrokSpeechConfig,
    language: &str,
    file_part: multipart::Part,
) -> multipart::Form {
    let mut form = multipart::Form::new().part("file", file_part);

    let language = language.trim();
    let has_language = !language.is_empty() && language != "auto";
    if has_language {
        form = form.text("language", language.to_string());
        // Inverse text normalization requires a concrete language.
        if config.text_formatting {
            form = form.text("format", "true");
        }
    }
    if config.filler_words {
        form = form.text("filler_words", "true");
    }
    for term in &config.keyterms {
        form = form.text("keyterm", term.clone());
    }

    form
}

fn validate_grok_config(config: &GrokSpeechConfig) -> Result<()> {
    if config.api_key.trim().is_empty() {
        return Err(anyhow!("Grok API key is not configured"));
    }
    Ok(())
}

/// 0.2 s of 16 kHz mono 16-bit silence, used as a connection probe.
fn silent_wav() -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut cursor, spec).expect("in-memory WAV writer cannot fail");
        for _ in 0..3_200 {
            writer
                .write_sample(0i16)
                .expect("in-memory WAV write cannot fail");
        }
        writer
            .finalize()
            .expect("in-memory WAV finalize cannot fail");
    }
    cursor.into_inner()
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
        token_confidences: None,
        no_speech_prob: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_grok_config, GrokSpeechBackend};
    use crate::settings::ComputeMode;
    use crate::whisper_backend::TranscribeRequest;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn spawn_server(
        status_line: &'static str,
        response_body: &'static str,
    ) -> (String, mpsc::Receiver<String>) {
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
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status_line,
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        (format!("http://{addr}"), rx)
    }

    fn temp_audio_file() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let path = std::env::temp_dir().join(format!("buttervoice-grok-test-{ts}-{unique}.wav"));
        fs::write(&path, b"RIFFgrok").unwrap();
        path
    }

    fn request_for(path: PathBuf) -> TranscribeRequest {
        TranscribeRequest {
            request_id: "req-grok-1".to_string(),
            audio_path: path,
            model_id: "grok-stt".to_string(),
            language: "en".to_string(),
            compute_mode: ComputeMode::Auto,
            beam_size: 2,
            prompt: String::new(),
            no_speech_thold: 0.6,
            temperature: 0.0,
            temperature_inc: 0.2,
            threads: 0,
        }
    }

    fn config_for(base_url: String) -> super::GrokSpeechConfig {
        let mut config = build_grok_config(
            "xai-secret",
            true,
            false,
            &["ButterVoice".to_string(), "Tauri".to_string()],
        );
        config.base_url = base_url;
        config
    }

    #[test]
    fn transcribe_posts_multipart_to_stt_endpoint() {
        let (base_url, rx) = spawn_server(
            "200 OK",
            r#"{"text":"hello from grok","language":"en","duration":1.2,"words":[{"text":"hello","start":0.0,"end":0.4,"confidence":0.97},{"text":"from","start":0.4,"end":0.6,"confidence":0.91},{"text":"grok","start":0.6,"end":0.9,"confidence":0.88}]}"#,
        );
        let backend = GrokSpeechBackend;
        let audio = temp_audio_file();
        let response = backend
            .transcribe(&config_for(base_url), &request_for(audio.clone()))
            .unwrap();
        let raw_request = rx.recv().unwrap();

        assert!(raw_request.starts_with("POST /stt HTTP/1.1"));
        assert!(raw_request
            .to_ascii_lowercase()
            .contains("authorization: bearer xai-secret"));
        assert!(raw_request.contains("name=\"file\""));
        assert!(raw_request.contains("name=\"language\""));
        assert!(raw_request.contains("name=\"format\""));
        assert!(raw_request.contains("name=\"keyterm\""));
        assert!(raw_request.contains("ButterVoice"));
        assert!(!raw_request.contains("name=\"filler_words\""));
        assert!(!raw_request.contains("name=\"model\""));

        assert_eq!(response.text, "hello from grok");
        assert_eq!(response.backend.as_deref(), Some("remote/grok"));
        let confidences = response.token_confidences.unwrap();
        assert_eq!(confidences.len(), 3);
        assert_eq!(confidences[0].text, "hello");
        assert!((confidences[0].prob - 0.97).abs() < f32::EPSILON);

        let _ = fs::remove_file(audio);
    }

    #[test]
    fn transcribe_skips_format_without_language() {
        let (base_url, rx) = spawn_server("200 OK", r#"{"text":"ciao"}"#);
        let backend = GrokSpeechBackend;
        let audio = temp_audio_file();
        let mut request = request_for(audio.clone());
        request.language = "auto".to_string();
        let response = backend.transcribe(&config_for(base_url), &request).unwrap();
        let raw_request = rx.recv().unwrap();

        assert!(!raw_request.contains("name=\"language\""));
        assert!(!raw_request.contains("name=\"format\""));
        assert_eq!(response.text, "ciao");
        assert!(response.token_confidences.is_none());

        let _ = fs::remove_file(audio);
    }

    #[test]
    fn transcribe_maps_auth_failure() {
        let (base_url, _rx) = spawn_server(
            "401 Unauthorized",
            r#"{"error":{"message":"invalid api key"}}"#,
        );
        let backend = GrokSpeechBackend;
        let audio = temp_audio_file();
        let response = backend
            .transcribe(&config_for(base_url), &request_for(audio.clone()))
            .unwrap();

        assert_eq!(response.error_code.as_deref(), Some("REMOTE_AUTH_FAILED"));
        assert!(response.error_message.unwrap().contains("invalid api key"));

        let _ = fs::remove_file(audio);
    }

    #[test]
    fn transcribe_maps_rate_limit_to_provider_unavailable() {
        let (base_url, _rx) = spawn_server(
            "429 Too Many Requests",
            r#"{"error":{"message":"rate limited"}}"#,
        );
        let backend = GrokSpeechBackend;
        let audio = temp_audio_file();
        let response = backend
            .transcribe(&config_for(base_url), &request_for(audio.clone()))
            .unwrap();

        assert_eq!(
            response.error_code.as_deref(),
            Some("REMOTE_PROVIDER_UNAVAILABLE")
        );

        let _ = fs::remove_file(audio);
    }

    #[test]
    fn transcribe_requires_api_key() {
        let backend = GrokSpeechBackend;
        let audio = temp_audio_file();
        let mut config = config_for("http://127.0.0.1:1".to_string());
        config.api_key = String::new();
        let err = backend
            .transcribe(&config, &request_for(audio.clone()))
            .unwrap_err();
        assert!(err.to_string().contains("API key"));

        let _ = fs::remove_file(audio);
    }

    #[test]
    fn test_connection_accepts_bad_request_probe_response() {
        let (base_url, rx) = spawn_server(
            "400 Bad Request",
            r#"{"error":{"message":"audio too short"}}"#,
        );
        let backend = GrokSpeechBackend;
        let preview = backend.test_connection(&config_for(base_url)).unwrap();
        let raw_request = rx.recv().unwrap();

        assert!(raw_request.starts_with("POST /stt HTTP/1.1"));
        assert!(raw_request.contains("name=\"file\""));
        assert!(preview.contains("API reachable"));
    }

    #[test]
    fn test_connection_rejects_invalid_key() {
        let (base_url, _rx) = spawn_server(
            "401 Unauthorized",
            r#"{"error":{"message":"invalid api key"}}"#,
        );
        let backend = GrokSpeechBackend;
        let err = backend.test_connection(&config_for(base_url)).unwrap_err();
        assert!(err.to_string().contains("rejected the API key"));
    }

    #[test]
    fn build_grok_config_caps_and_filters_keyterms() {
        let mut terms: Vec<String> = (0..120).map(|i| format!("term-{i}")).collect();
        terms.push("x".repeat(51));
        terms.push("   ".to_string());
        let config = build_grok_config("key", true, false, &terms);
        assert_eq!(config.keyterms.len(), 100);
        assert!(config.keyterms.iter().all(|t| t.chars().count() <= 50));
    }

    #[test]
    fn backend_status_requires_api_key() {
        let backend = GrokSpeechBackend;
        let configured = build_grok_config("key", true, false, &[]);
        assert!(backend.backend_status(&configured).provider_ok);

        let missing = build_grok_config("", true, false, &[]);
        let status = backend.backend_status(&missing);
        assert!(!status.provider_ok);
        assert!(status.provider_error.unwrap().contains("not configured"));
    }
}
