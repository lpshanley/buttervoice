//! Results read per kind of sample, per microphone, per speaker or by any
//! other label. One average over a whole suite hides exactly what the kinds
//! and the speakers were recorded to show.

use std::time::Duration;

use buttervoice_core::capture::InputKind;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::TranscriptionFailure;
use buttervoice_samples::Kind;

use crate::scoring::word_errors;

#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub kind: Kind,
    pub language: Language,
    pub speaker: String,
    pub input: InputKind,
    /// What the person said. Empty when they said nothing on purpose.
    pub said: String,
    pub heard: Result<String, TranscriptionFailure>,
    /// How long the recording is.
    pub audio: Duration,
    /// How long transcribing it took.
    pub took: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub samples: usize,
    /// Wrong, missed and added words over all the words said, across the
    /// samples together. `None` when nothing was said in any of them.
    pub word_error_rate: Option<f64>,
    /// Samples in which nothing was said and something was written anyway.
    pub invented_text: usize,
    pub failures: usize,
    /// Seconds of audio transcribed per second of work.
    pub times_real_time: Option<f64>,
    pub median_took: Duration,
}

pub fn summarise<'a>(scored: impl IntoIterator<Item = &'a Scored>) -> Summary {
    let (mut samples, mut errors, mut said, mut invented_text, mut failures) = (0, 0, 0, 0, 0);
    let (mut audio, mut took) = (Duration::ZERO, Vec::new());
    for sample in scored {
        samples += 1;
        audio += sample.audio;
        took.push(sample.took);
        let heard = match &sample.heard {
            Ok(text) => text.as_str(),
            Err(_) => {
                failures += 1;
                ""
            }
        };
        let (sample_errors, sample_said) = word_errors(sample.language, &sample.said, heard);
        errors += sample_errors;
        said += sample_said;
        if sample_said == 0 && !heard.trim().is_empty() {
            invented_text += 1;
        }
    }
    Summary {
        samples,
        word_error_rate: (said > 0).then(|| errors as f64 / said as f64),
        invented_text,
        failures,
        times_real_time: cost(audio, &took),
        median_took: median(took),
    }
}

fn cost(audio: Duration, took: &[Duration]) -> Option<f64> {
    let work: Duration = took.iter().sum();
    (!work.is_zero()).then(|| audio.as_nanos() as f64 / work.as_nanos() as f64)
}

fn median(mut took: Vec<Duration>) -> Duration {
    took.sort();
    took.get(took.len() / 2).copied().unwrap_or_default()
}

/// One summary per kind recorded, in the kinds' usual order.
pub fn by_kind(scored: &[Scored]) -> Vec<(Kind, Summary)> {
    Kind::ALL
        .into_iter()
        .filter(|kind| scored.iter().any(|sample| sample.kind == *kind))
        .map(|kind| {
            let of_this_kind = scored.iter().filter(|sample| sample.kind == kind);
            (kind, summarise(of_this_kind))
        })
        .collect()
}

/// One summary per label, in the order the labels first appear.
pub fn by<K: PartialEq + Clone>(
    scored: &[Scored],
    label: impl Fn(&Scored) -> K,
) -> Vec<(K, Summary)> {
    let mut labels: Vec<K> = Vec::new();
    for sample in scored {
        let this = label(sample);
        if !labels.contains(&this) {
            labels.push(this);
        }
    }
    labels
        .into_iter()
        .map(|this| {
            let labelled = scored.iter().filter(|sample| label(sample) == this);
            let summary = summarise(labelled);
            (this, summary)
        })
        .collect()
}
