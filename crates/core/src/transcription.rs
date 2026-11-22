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
