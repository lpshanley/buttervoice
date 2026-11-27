//! What was dictated, kept as one JSON line per dictation until cleared.

pub mod usage;

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use buttervoice_core::key_dictation::FinishedDictation;
use buttervoice_core::transcription::Transcription;
use buttervoice_core::typing::Typing;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// The app a dictation was typed into.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct App {
    pub name: String,
    pub bundle_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub text: String,
    /// Seconds since 1970.
    pub at: u64,
    pub language: Option<String>,
    pub backend: Option<String>,
    pub app: Option<App>,
    /// What was heard, when thinking sounds were taken out of `text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heard: Option<String>,
}

/// What is kept of a finished dictation, if anything: only text that was
/// to be typed for the person.
pub fn entry_for(dictation: &FinishedDictation, at: u64, app: Option<App>) -> Option<Entry> {
    Some(Entry {
        text: text_to_keep(dictation)?.to_string(),
        at,
        language: dictation
            .language
            .map(|language| language.tag().to_string()),
        backend: dictation.backend.map(str::to_string),
        app,
        heard: dictation
            .dropped_fillers
            .as_ref()
            .map(|dropped| dropped.heard.clone()),
    })
}

/// The text of a dictation that was to be typed for the person. Sample takes
/// and blank results never were.
fn text_to_keep(dictation: &FinishedDictation) -> Option<&str> {
    let Transcription::Succeeded(transcript) = &dictation.transcription else {
        return None;
    };
    (dictation.typing != Typing::NotAttempted).then_some(transcript.text.as_str())
}

pub struct History {
    file: PathBuf,
    /// Held while the file is written, so a search never reads half a line.
    writing: Mutex<()>,
}

impl History {
    pub fn open(file: PathBuf) -> Self {
        Self {
            file,
            writing: Mutex::new(()),
        }
    }

    pub fn add(&self, entry: &Entry) -> std::io::Result<()> {
        let mut line = serde_json::to_string(entry)?;
        line.push('\n');
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)?
            .write_all(line.as_bytes())
    }

    pub fn clear(&self) -> std::io::Result<()> {
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        match std::fs::remove_file(&self.file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            cleared => cleared,
        }
    }

    /// Newest first. An entry matches when it holds every word of the query,
    /// whatever their accents or capitals.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Entry> {
        let words: Vec<String> = folded(query)
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let contents = {
            let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
            std::fs::read_to_string(&self.file).unwrap_or_default()
        };
        contents
            .lines()
            .rev()
            .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
            .filter(|entry| {
                let text = folded(&entry.text);
                words.iter().all(|word| text.contains(word.as_str()))
            })
            .take(limit)
            .collect()
    }
}

/// "Lançar" and "lancar" fold to the same text.
fn folded(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}
