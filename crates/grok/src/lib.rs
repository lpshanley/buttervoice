use std::time::Duration;

use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::transcription::{Transcript, TranscriptionFailure};
use reqwest::blocking::{Client, multipart};
use serde::Deserialize;

pub const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const LANGUAGE: &str = "en";

#[derive(Debug, thiserror::Error)]
#[error("the HTTP client could not be set up")]
pub struct SetupError(#[source] reqwest::Error);

/// Holds one HTTP client for its whole life, so later requests reuse the
/// connection instead of paying for a new TLS handshake each time.
pub struct GrokTranscriber {
    client: Client,
    endpoint: String,
    api_key: String,
}

#[derive(Deserialize)]
struct SttResponse {
    text: String,
}

impl GrokTranscriber {
    pub fn new(base_url: &str, api_key: String) -> Result<Self, SetupError> {
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(SetupError)?;
        Ok(Self {
            client,
            endpoint: format!("{}/stt", base_url.trim_end_matches('/')),
            api_key,
        })
    }

    pub fn transcribe(&self, audio: &CapturedAudio) -> Result<Transcript, TranscriptionFailure> {
        let file = multipart::Part::bytes(audio.to_wav())
            .file_name("dictation.wav")
            .mime_str("audio/wav")
            .map_err(|_| TranscriptionFailure::Rejected)?;
        // `file` goes last: the service may ignore fields that come after it.
        let form = multipart::Form::new()
            .text("language", LANGUAGE)
            .text("format", "true")
            .part("file", file);
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .map_err(|_| TranscriptionFailure::Unreachable)?;
        match response.status().as_u16() {
            200..=299 => response
                .json::<SttResponse>()
                .map(|heard| Transcript {
                    text: heard.text.trim().to_string(),
                })
                .map_err(|_| TranscriptionFailure::Unavailable),
            401 | 403 => Err(TranscriptionFailure::Unauthorized),
            408 | 429 | 500..=599 => Err(TranscriptionFailure::Unavailable),
            _ => Err(TranscriptionFailure::Rejected),
        }
    }
}
