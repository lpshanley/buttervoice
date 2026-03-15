use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Manages the frequency dictionary for spell checking.
/// Supports a bundled compact dictionary and an optional extended dictionary.
#[allow(dead_code)]
pub struct DictionaryManager {
    /// Word → frequency entries.
    entries: HashMap<String, u64>,
    /// Path where extended dictionary would be cached.
    extended_dict_path: PathBuf,
    /// Whether the extended dictionary is loaded.
    extended_loaded: bool,
}

#[allow(dead_code)]
impl DictionaryManager {
    pub fn new(base_dir: &Path) -> Result<Self> {
        let extended_dict_path = base_dir.join("dictionaries").join("en_extended.txt");
        let mut manager = Self {
            entries: HashMap::new(),
            extended_dict_path,
            extended_loaded: false,
        };

        // Try to load the extended dictionary first, fall back to bundled
        if manager.extended_dict_path.exists() {
            match manager.load_dictionary_file(&manager.extended_dict_path.clone()) {
                Ok(count) => {
                    eprintln!("loaded extended dictionary: {count} entries");
                    manager.extended_loaded = true;
                }
                Err(err) => {
                    eprintln!("failed to load extended dictionary, using built-in: {err:#}");
                    manager.load_builtin_dictionary();
                }
            }
        } else {
            manager.load_builtin_dictionary();
        }

        Ok(manager)
    }

    /// Load the built-in compact dictionary (embedded in the binary).
    fn load_builtin_dictionary(&mut self) {
        // Built-in dictionary: English words with frequencies from Google Web Trillion Word Corpus.
        // Filtered to ~99k entries with frequency >= 100,000.
        let builtin = include_str!("../../resources/dictionaries/en_compact.txt");
        self.parse_frequency_data(builtin);
        eprintln!(
            "loaded built-in compact dictionary: {} entries",
            self.entries.len()
        );

        // Layer on technical vocabulary so common programming terms are
        // recognized as valid words and never "corrected".
        let tech = include_str!("../../resources/dictionaries/tech_vocab.txt");
        let before = self.entries.len();
        self.parse_frequency_data(tech);
        let added = self.entries.len() - before;
        if added > 0 {
            eprintln!("loaded built-in tech vocabulary: {added} new entries");
        }
    }

    /// Load a dictionary file in "word frequency" format (one per line, space-separated).
    fn load_dictionary_file(&mut self, path: &Path) -> Result<usize> {
        let content = std::fs::read_to_string(path).context("failed to read dictionary file")?;
        self.parse_frequency_data(&content);
        Ok(self.entries.len())
    }

    /// Parse frequency data in "word frequency" format.
    fn parse_frequency_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut parts = line.splitn(2, |c: char| c.is_whitespace());
            if let Some(word) = parts.next() {
                let word = word.to_lowercase();
                if word.len() < 2 || !word.chars().all(|c| c.is_alphabetic() || c == '\'') {
                    continue;
                }
                let freq: u64 = parts
                    .next()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1);
                self.entries.insert(word, freq);
            }
        }
    }

    /// Get all dictionary entries.
    pub fn entries(&self) -> &HashMap<String, u64> {
        &self.entries
    }

    /// Add custom words to the dictionary with high frequency.
    ///
    /// Uses 500M — high enough to be strongly preferred by SymSpell's
    /// frequency-based ranking, but below the very top of the distribution
    /// (e.g. "the" at 23B) so it doesn't completely dominate.
    pub fn add_custom_words(&mut self, words: &[String]) {
        for word in words {
            let word = word.trim().to_lowercase();
            if !word.is_empty() {
                self.entries.insert(word, 500_000_000);
            }
        }
    }

    /// Check if a word exists in the dictionary.
    pub fn contains(&self, word: &str) -> bool {
        self.entries.contains_key(&word.to_lowercase())
    }

    /// Whether the extended dictionary is loaded.
    pub fn is_extended(&self) -> bool {
        self.extended_loaded
    }

    /// Path where the extended dictionary should be saved.
    pub fn extended_dict_path(&self) -> &Path {
        &self.extended_dict_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frequency_data() {
        let mut manager = DictionaryManager {
            entries: HashMap::new(),
            extended_dict_path: PathBuf::from("/tmp/test_dict.txt"),
            extended_loaded: false,
        };

        manager.parse_frequency_data("the 100000\ncat 5000\ndog 4000\n");
        assert_eq!(manager.entries.len(), 3);
        assert_eq!(manager.entries.get("the"), Some(&100000));
        assert_eq!(manager.entries.get("cat"), Some(&5000));
    }

    #[test]
    fn adds_custom_words() {
        let mut manager = DictionaryManager {
            entries: HashMap::new(),
            extended_dict_path: PathBuf::from("/tmp/test_dict.txt"),
            extended_loaded: false,
        };

        manager.add_custom_words(&["Kubernetes".to_string(), "Tauri".to_string()]);
        assert!(manager.contains("kubernetes"));
        assert!(manager.contains("tauri"));
        // Custom words get high frequency (500M) so SymSpell prefers them
        assert_eq!(manager.entries.get("kubernetes"), Some(&500_000_000));
    }

    #[test]
    fn skips_comments_and_empty_lines() {
        let mut manager = DictionaryManager {
            entries: HashMap::new(),
            extended_dict_path: PathBuf::from("/tmp/test_dict.txt"),
            extended_loaded: false,
        };

        manager.parse_frequency_data("# comment\n\nthe 100\n");
        assert_eq!(manager.entries.len(), 1);
    }
}
