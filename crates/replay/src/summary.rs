//! Results read per kind of sample. One average over a whole set hides
//! exactly what the kinds were recorded to show.

use buttervoice_core::transcription::TranscriptionFailure;
use buttervoice_samples::Kind;

use crate::scoring::word_errors;

#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub kind: Kind,
    /// What the person said. Empty when they said nothing on purpose.
    pub said: String,
    pub heard: Result<String, TranscriptionFailure>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KindSummary {
    pub kind: Kind,
    pub samples: usize,
    /// Wrong, missed and added words over all the words said, across the
    /// kind's samples together. `None` when nothing was said in any of them.
    pub word_error_rate: Option<f64>,
    /// Samples in which nothing was said and something was written anyway.
    pub invented_text: usize,
    pub failures: usize,
}

pub fn summarise(scored: &[Scored]) -> Vec<KindSummary> {
    Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let of_this_kind: Vec<&Scored> =
                scored.iter().filter(|sample| sample.kind == kind).collect();
            if of_this_kind.is_empty() {
                return None;
            }
            let (mut errors, mut said, mut invented_text, mut failures) = (0, 0, 0, 0);
            for sample in &of_this_kind {
                let heard = match &sample.heard {
                    Ok(text) => text.as_str(),
                    Err(_) => {
                        failures += 1;
                        ""
                    }
                };
                let (sample_errors, sample_said) = word_errors(&sample.said, heard);
                errors += sample_errors;
                said += sample_said;
                if sample_said == 0 && !heard.trim().is_empty() {
                    invented_text += 1;
                }
            }
            Some(KindSummary {
                kind,
                samples: of_this_kind.len(),
                word_error_rate: (said > 0).then(|| errors as f64 / said as f64),
                invented_text,
                failures,
            })
        })
        .collect()
}
