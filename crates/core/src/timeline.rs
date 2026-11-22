use std::time::Duration;

/// Monotonic clock reading. Where the OS event or audio callback carries its
/// own timestamp, use that, not the time our thread got around to handling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(u64);

impl Timestamp {
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    KeyDown,
    FirstSample,
    KeyUp,
    TextLanded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub key_down_to_first_sample: Option<Duration>,
    pub key_up_to_text_landed: Option<Duration>,
}

#[derive(Debug, Default)]
pub struct Timeline {
    key_down: Option<Timestamp>,
    first_sample: Option<Timestamp>,
    key_up: Option<Timestamp>,
    text_landed: Option<Timestamp>,
}

impl Timeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark(&mut self, mark: Mark, at: Timestamp) {
        match mark {
            Mark::KeyDown => self.key_down = Some(at),
            Mark::FirstSample => self.first_sample = Some(at),
            Mark::KeyUp => self.key_up = Some(at),
            Mark::TextLanded => self.text_landed = Some(at),
        }
    }

    pub fn summary(&self) -> Summary {
        Summary {
            key_down_to_first_sample: between(self.key_down, self.first_sample),
            key_up_to_text_landed: between(self.key_up, self.text_landed),
        }
    }
}

fn between(from: Option<Timestamp>, to: Option<Timestamp>) -> Option<Duration> {
    // Saturates: a warm microphone stream captures its first sample before
    // the key goes down.
    Some(Duration::from_nanos(to?.0.saturating_sub(from?.0)))
}
