//! How much is dictated each day: counts only, never the words themselves.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use buttervoice_core::key_dictation::FinishedDictation;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Day {
    pub dictations: u64,
    pub words: u64,
    pub spoken_ms: u64,
}

pub struct Usage {
    file: PathBuf,
    /// Held from reading the days to saving them, so no count is lost.
    changing: Mutex<()>,
}

impl Usage {
    pub fn open(file: PathBuf) -> Self {
        Self {
            file,
            changing: Mutex::new(()),
        }
    }

    /// `day` is the local date, such as `2026-11-22`.
    pub fn count(&self, day: &str, dictation: &FinishedDictation) -> std::io::Result<()> {
        let Some(text) = crate::text_to_keep(dictation) else {
            return Ok(());
        };
        let _changing = self.changing.lock().unwrap_or_else(PoisonError::into_inner);
        let mut days = self.read();
        let counted = days.entry(day.to_string()).or_default();
        counted.dictations += 1;
        counted.words += text.split_whitespace().count() as u64;
        counted.spoken_ms += dictation
            .audio
            .map(|audio| audio.duration.as_millis() as u64)
            .unwrap_or_default();
        // Written beside the file and renamed over it, so a crash part-way
        // leaves the old counts rather than none.
        let partial = self.file.with_extension("partial");
        std::fs::write(&partial, serde_json::to_string(&days)?)?;
        std::fs::rename(partial, &self.file)
    }

    /// Every day with a dictation, keyed by date.
    pub fn days(&self) -> BTreeMap<String, Day> {
        let _changing = self.changing.lock().unwrap_or_else(PoisonError::into_inner);
        self.read()
    }

    fn read(&self) -> BTreeMap<String, Day> {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }
}

/// How long typing `words` at `words_per_minute` would take, less the time
/// spent saying them.
pub fn time_saved(words: u64, spoken_ms: u64, words_per_minute: u32) -> std::time::Duration {
    let typing_ms = words * 60_000 / u64::from(words_per_minute.max(1));
    std::time::Duration::from_millis(typing_ms.saturating_sub(spoken_ms))
}
