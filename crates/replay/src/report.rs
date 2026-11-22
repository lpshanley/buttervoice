use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::transcription::{Transcript, TranscriptionFailure};

use crate::recordings::Recording;
use crate::scoring::word_error_rate;

#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub recording: String,
    pub reference: String,
    pub outcome: Result<String, TranscriptionFailure>,
    pub word_error_rate: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub heard: Vec<Heard>,
}

impl Report {
    pub fn mean_word_error_rate(&self) -> f64 {
        let total: f64 = self.heard.iter().map(|heard| heard.word_error_rate).sum();
        total / self.heard.len() as f64
    }

    /// Recordings the service answered with no text at all.
    pub fn blanks(&self) -> usize {
        self.heard
            .iter()
            .filter(|heard| matches!(&heard.outcome, Ok(text) if text.is_empty()))
            .count()
    }

    pub fn failures(&self) -> usize {
        self.heard
            .iter()
            .filter(|heard| heard.outcome.is_err())
            .count()
    }
}

/// Transcribes each recording and scores it against the sentence that was
/// read for it. A request that fails scores as if nothing had been heard.
pub fn replay(
    recordings: &[Recording],
    references: &[String],
    transcribe: impl Fn(&CapturedAudio) -> Result<Transcript, TranscriptionFailure>,
) -> Report {
    let heard = recordings
        .iter()
        .zip(references)
        .map(|(recording, reference)| {
            let outcome = transcribe(&recording.audio).map(|transcript| transcript.text);
            let text = outcome.as_deref().unwrap_or("");
            Heard {
                recording: recording.name.clone(),
                reference: reference.clone(),
                word_error_rate: word_error_rate(reference, text),
                outcome,
            }
        })
        .collect();
    Report { heard }
}
