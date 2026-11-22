#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionFailure {
    /// There is no transcription service set up, for example no API key.
    NotConfigured,
    Unauthorized,
    /// The service could not take the request right now.
    Unavailable,
    /// The service refused this particular request.
    Rejected,
    Unreachable,
    /// The speech engine on this machine could not transcribe the audio.
    EngineFailed,
}

/// Where a dictation's audio is turned into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// On this machine. Nothing about the dictation leaves it.
    Local,
    Grok,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transcription {
    /// Nothing worth transcribing was captured.
    NotAttempted,
    Succeeded(Transcript),
    Failed(TranscriptionFailure),
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Local => "local",
            Provider::Grok => "grok",
        }
    }

    /// The provider a saved choice stands for. With no choice, or one this
    /// version does not know, dictation stays on this machine.
    pub fn from_setting(saved: Option<&str>) -> Provider {
        match saved {
            Some("grok") => Provider::Grok,
            _ => Provider::Local,
        }
    }

    /// Runs only this provider's set-up. In local mode nothing of the cloud
    /// service is built, and its API key is never looked up.
    pub fn set_up<T>(self, local: impl FnOnce() -> T, cloud: impl FnOnce() -> T) -> T {
        match self {
            Provider::Local => local(),
            Provider::Grok => cloud(),
        }
    }
}
