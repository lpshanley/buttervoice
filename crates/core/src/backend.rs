//! The ways a dictation can be transcribed, described as data, so that
//! adding one is an entry here and not a change through the app.

use crate::language::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Nothing about a dictation leaves the computer.
    ThisMac,
    Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Model {
    pub file: &'static str,
    pub source: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    pub licence: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backend {
    pub id: &'static str,
    pub place: Place,
    pub model: Option<Model>,
    pub speaks: &'static [Language],
    pub vetted_for: &'static [Language],
}

/// In order of preference: among backends that fit a person equally, the
/// earlier one is recommended.
pub const CATALOG: [Backend; 2] = [
    // Chosen by benchmark on real dictations: as accurate as the
    // alternatives tried and several times faster.
    Backend {
        id: "parakeet-tdt-0.6b-v2",
        place: Place::ThisMac,
        model: Some(Model {
            file: "parakeet-tdt-0.6b-v2-Q8_0.gguf",
            source: "https://huggingface.co/handy-computer/parakeet-tdt-0.6b-v2-gguf/resolve/58239cf6d0ecb0af921e7936c1e5e0a8dba91dbb/parakeet-tdt-0.6b-v2-Q8_0.gguf",
            sha256: "f0d0e99cebb6d3b83f1f7069b82b5d3c2e39a54545b0da039cb4bafd9c4e5caa",
            bytes: 729_574_912,
            licence: "CC-BY-4.0",
        }),
        speaks: &[Language::English],
        vetted_for: &[Language::English],
    },
    Backend {
        id: "grok",
        place: Place::Cloud,
        model: None,
        speaks: &[Language::English, Language::BrazilianPortuguese],
        vetted_for: &[Language::English],
    },
];

pub fn find(id: &str) -> Option<&'static Backend> {
    CATALOG.iter().find(|backend| backend.id == id)
}

/// The backend to recommend to someone who wants their dictations kept on
/// this Mac. Speaking a language is the model's claim; being vetted for it
/// means it was benchmarked on real samples, and only that earns a
/// recommendation.
pub fn best_local_fit(catalog: &[Backend], language: Language) -> Option<&Backend> {
    catalog
        .iter()
        .find(|backend| backend.place == Place::ThisMac && backend.vetted_for.contains(&language))
}
