use std::time::Duration;

use crate::DictationId;

/// Monotonic clock reading. Where the OS event or audio callback carries its
/// own timestamp, use that, not the time our thread got around to handling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(u64);

impl Timestamp {
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    pub fn saturating_duration_since(self, earlier: Timestamp) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    KeyDown,
    FirstSample,
    FirstSound,
    KeyUp,
    CaptureStopped,
    TranscriptReady,
    TextTyped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub dictation_id: DictationId,
    pub key_held: Option<Duration>,
    pub key_down_to_first_sample: Option<Duration>,
    pub key_down_to_first_sound: Option<Duration>,
    pub key_up_to_capture_stopped: Option<Duration>,
    pub transcription: Option<Duration>,
    pub key_up_to_text_typed: Option<Duration>,
}

#[derive(Debug)]
pub struct Timeline {
    dictation_id: DictationId,
    key_down: Option<Timestamp>,
    first_sample: Option<Timestamp>,
    first_sound: Option<Timestamp>,
    key_up: Option<Timestamp>,
    capture_stopped: Option<Timestamp>,
    transcript_ready: Option<Timestamp>,
    text_typed: Option<Timestamp>,
}

impl Timeline {
    pub fn new(dictation_id: DictationId) -> Self {
        Self {
            dictation_id,
            key_down: None,
            first_sample: None,
            first_sound: None,
            key_up: None,
            capture_stopped: None,
            transcript_ready: None,
            text_typed: None,
        }
    }

    pub fn mark(&mut self, mark: Mark, at: Timestamp) {
        match mark {
            Mark::KeyDown => self.key_down = Some(at),
            Mark::FirstSample => self.first_sample = Some(at),
            Mark::FirstSound => self.first_sound = Some(at),
            Mark::KeyUp => self.key_up = Some(at),
            Mark::CaptureStopped => self.capture_stopped = Some(at),
            Mark::TranscriptReady => self.transcript_ready = Some(at),
            Mark::TextTyped => self.text_typed = Some(at),
        }
    }

    pub fn summary(&self) -> Summary {
        Summary {
            dictation_id: self.dictation_id,
            key_held: between(self.key_down, self.key_up),
            key_down_to_first_sample: between(self.key_down, self.first_sample),
            key_down_to_first_sound: between(self.key_down, self.first_sound),
            key_up_to_capture_stopped: between(self.key_up, self.capture_stopped),
            transcription: between(self.capture_stopped, self.transcript_ready),
            key_up_to_text_typed: between(self.key_up, self.text_typed),
        }
    }
}

fn between(from: Option<Timestamp>, to: Option<Timestamp>) -> Option<Duration> {
    // Saturates: a warm microphone stream captures its first sample before
    // the key goes down.
    Some(to?.saturating_duration_since(from?))
}
