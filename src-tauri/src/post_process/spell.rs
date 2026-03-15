use std::collections::{HashMap, HashSet};

use symspell::{AsciiStringStrategy, SymSpell, SymSpellBuilder, Verbosity};

use super::dictionary::DictionaryManager;
use super::{PipelineStage, TextEdit};

pub struct SpellChecker {
    symspell: SymSpell<AsciiStringStrategy>,
    /// Immutable base dictionary frequencies loaded at startup.
    base_word_freqs: HashMap<String, u64>,
    /// Word → frequency for computing frequency-aware confidence.
    word_freqs: HashMap<String, u64>,
    /// Words explicitly added by the user (never corrected).
    custom_words: HashSet<String>,
}

const CUSTOM_WORD_FREQ: u64 = 500_000_000;

impl SpellChecker {
    fn build_symspell_from_freqs(entries: &HashMap<String, u64>) -> SymSpell<AsciiStringStrategy> {
        let mut symspell: SymSpell<AsciiStringStrategy> = SymSpellBuilder::default()
            .max_dictionary_edit_distance(2)
            .prefix_length(7)
            .count_threshold(1)
            .build()
            .expect("default symspell builder should not fail");

        for (word, freq) in entries {
            if !is_symspell_term_safe(word) {
                continue;
            }
            // Use tab as the separator so terms cannot break count parsing
            // when users provide entries that include spaces.
            let count = (*freq).min(i64::MAX as u64);
            let line = format!("{word}\t{count}");
            symspell.load_dictionary_line(&line, 0, 1, "\t");
        }

        symspell
    }

    /// Create an empty spell checker (no dictionary loaded).
    pub fn new_empty() -> Self {
        Self {
            symspell: Self::build_symspell_from_freqs(&HashMap::new()),
            base_word_freqs: HashMap::new(),
            word_freqs: HashMap::new(),
            custom_words: HashSet::new(),
        }
    }

    pub fn new(dict_manager: &DictionaryManager) -> anyhow::Result<Self> {
        let word_freqs = dict_manager.entries().clone();
        let symspell = Self::build_symspell_from_freqs(&word_freqs);

        eprintln!(
            "spell checker initialized with {} dictionary entries",
            dict_manager.entries().len()
        );

        Ok(Self {
            symspell,
            base_word_freqs: word_freqs.clone(),
            word_freqs,
            custom_words: HashSet::new(),
        })
    }

    /// Update custom words in the dictionary.
    pub fn update_custom_words(&mut self, words: &[String]) {
        self.custom_words.clear();
        for word in words {
            // Support accidental phrase entries by extracting valid tokens.
            for token in word.split_whitespace() {
                let normalized = token.trim().to_lowercase();
                if is_spell_token(&normalized) {
                    self.custom_words.insert(normalized);
                }
            }
        }

        self.word_freqs = self.base_word_freqs.clone();
        for word in &self.custom_words {
            self.word_freqs.insert(word.clone(), CUSTOM_WORD_FREQ);
        }

        self.symspell = Self::build_symspell_from_freqs(&self.word_freqs);
    }

    /// Look up the frequency of a word. Returns 0 for unknown words.
    fn word_frequency(&self, word: &str) -> u64 {
        self.word_freqs.get(word).copied().unwrap_or(0)
    }

    /// Compute confidence combining edit distance and frequency evidence.
    fn compute_confidence(&self, original: &str, suggestion_count: i64, distance: i64) -> f32 {
        let distance_score: f64 = match distance {
            1 => 0.9,
            2 => 0.7,
            _ => 0.4,
        };

        // Frequency ratio in log space: how much more frequent is the suggestion?
        let original_freq = self.word_frequency(original).max(1) as f64;
        let suggestion_freq = suggestion_count.max(1) as f64;
        let freq_ratio = (suggestion_freq.ln() - original_freq.ln()) / 10.0_f64.ln();

        // Clamp to [-1, 1] then scale to [0, 1]
        let freq_confidence = (freq_ratio.clamp(-1.0, 1.0) + 1.0) / 2.0;

        // Combined: 70% edit distance, 30% frequency evidence
        (0.7 * distance_score + 0.3 * freq_confidence) as f32
    }

    /// Process text: find and suggest corrections for misspelled words.
    pub fn process(&self, text: &str) -> Vec<TextEdit> {
        let mut edits = Vec::new();
        let words = self.word_spans(text);

        for (offset, word) in &words {
            // Skip words that are:
            // - Too short (1-2 chars)
            // - All uppercase (likely acronyms)
            // - Contain digits
            // - Start with uppercase in non-sentence-start position (likely proper nouns)
            if word.len() <= 2
                || word.chars().all(|c| c.is_uppercase())
                || word.chars().any(|c| c.is_ascii_digit())
            {
                continue;
            }

            // Skip likely proper nouns (capitalized mid-sentence)
            if *offset > 0 && word.chars().next().is_some_and(|c| c.is_uppercase()) {
                // Check if preceded by sentence-ending punctuation + space
                let before = &text[..*offset];
                let trimmed_before = before.trim_end();
                if !trimmed_before.is_empty() {
                    let last_char = trimmed_before.chars().last().unwrap();
                    if !matches!(last_char, '.' | '!' | '?') {
                        // Capitalized word mid-sentence — likely a proper noun, skip
                        continue;
                    }
                }
            }

            let lower = word.to_lowercase();

            // Never correct words the user explicitly added.
            if self.custom_words.contains(&lower) {
                continue;
            }

            let suggestions = self.symspell.lookup(&lower, Verbosity::Top, 2);

            if let Some(suggestion) = suggestions.first() {
                if suggestion.distance > 0 && suggestion.term != lower {
                    // Do not degrade valid contractions by stripping apostrophes,
                    // e.g. "don't" -> "dont".
                    if drops_apostrophe_from_contraction(&lower, &suggestion.term) {
                        continue;
                    }

                    // Preserve original casing pattern
                    let replacement = match_case(word, &suggestion.term);

                    // Combined confidence: edit distance + frequency evidence
                    let confidence =
                        self.compute_confidence(&lower, suggestion.count, suggestion.distance);

                    edits.push(TextEdit {
                        offset: *offset,
                        length: word.len(),
                        replacement,
                        source: PipelineStage::SpellCorrection,
                        confidence,
                        rule_id: format!("spell_ed{}", suggestion.distance),
                    });
                }
            }
        }

        edits
    }

    /// Split text into word spans: (byte_offset, word_str).
    fn word_spans<'a>(&self, text: &'a str) -> Vec<(usize, &'a str)> {
        let mut spans = Vec::new();
        let mut word_start = None;

        for (i, ch) in text.char_indices() {
            if ch.is_alphabetic() || ch == '\'' {
                if word_start.is_none() {
                    word_start = Some(i);
                }
            } else if let Some(start) = word_start {
                let word = &text[start..i];
                let trimmed = word.trim_matches('\'');
                if !trimmed.is_empty() {
                    spans.push((
                        start + (word.len() - word.trim_start_matches('\'').len()),
                        trimmed,
                    ));
                }
                word_start = None;
            }
        }

        if let Some(start) = word_start {
            let word = &text[start..];
            let trimmed = word.trim_matches('\'');
            if !trimmed.is_empty() {
                spans.push((
                    start + (word.len() - word.trim_start_matches('\'').len()),
                    trimmed,
                ));
            }
        }

        spans
    }
}

fn is_spell_token(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_alphabetic() || c == '\'')
}

fn is_symspell_term_safe(word: &str) -> bool {
    !word.is_empty() && !word.chars().any(|c| c == '\t' || c == '\r' || c == '\n')
}

fn drops_apostrophe_from_contraction(original: &str, suggestion: &str) -> bool {
    if !original.contains('\'') || suggestion.contains('\'') {
        return false;
    }

    let original_without_apostrophes: String = original.chars().filter(|c| *c != '\'').collect();
    if original_without_apostrophes != suggestion {
        return false;
    }

    looks_like_contraction(original)
}

fn looks_like_contraction(word: &str) -> bool {
    let lower = word.to_lowercase();
    lower.ends_with("n't")
        || lower.ends_with("'re")
        || lower.ends_with("'ve")
        || lower.ends_with("'ll")
        || lower.ends_with("'d")
        || lower.ends_with("'m")
        || lower.ends_with("'s")
}

/// Match the casing pattern of the original word to the replacement.
fn match_case(original: &str, replacement: &str) -> String {
    if original.chars().all(|c| c.is_uppercase()) {
        // ALL CAPS
        replacement.to_uppercase()
    } else if original.chars().next().is_some_and(|c| c.is_uppercase()) {
        // Title Case
        let mut chars = replacement.chars();
        match chars.next() {
            Some(first) => {
                let upper: String = first.to_uppercase().collect();
                format!("{}{}", upper, chars.as_str())
            }
            None => replacement.to_string(),
        }
    } else {
        replacement.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_case_preserves_lowercase() {
        assert_eq!(match_case("hello", "world"), "world");
    }

    #[test]
    fn match_case_preserves_titlecase() {
        assert_eq!(match_case("Hello", "world"), "World");
    }

    #[test]
    fn match_case_preserves_uppercase() {
        assert_eq!(match_case("HELLO", "world"), "WORLD");
    }

    // ── Confidence formula tests ──

    fn checker_with_freqs(entries: &[(&str, u64)]) -> SpellChecker {
        let mut word_freqs = HashMap::new();
        for (word, freq) in entries {
            word_freqs.insert(word.to_string(), *freq);
        }
        SpellChecker {
            symspell: SymSpellBuilder::default()
                .max_dictionary_edit_distance(2)
                .prefix_length(7)
                .count_threshold(1)
                .build()
                .unwrap(),
            base_word_freqs: HashMap::new(),
            word_freqs,
            custom_words: HashSet::new(),
        }
    }

    #[test]
    fn confidence_higher_when_suggestion_much_more_frequent() {
        let checker = checker_with_freqs(&[("teh", 100)]);
        // "the" has freq 23B — much more frequent than "teh" at 100
        let conf = checker.compute_confidence("teh", 23_000_000_000, 1);
        // Distance-1 with huge frequency advantage → high confidence
        assert!(conf > 0.75, "expected high confidence, got {conf}");
    }

    #[test]
    fn confidence_lower_when_suggestion_similar_frequency() {
        let checker = checker_with_freqs(&[("git", 50_000_000)]);
        // "get" has similar frequency — less confident this is a real error
        let conf = checker.compute_confidence("git", 80_000_000, 1);
        // Distance-1 but similar frequency → moderate confidence
        assert!(conf < 0.85, "expected moderate confidence, got {conf}");
        assert!(conf > 0.5, "expected above 0.5, got {conf}");
    }

    #[test]
    fn confidence_very_low_for_distance2_with_no_freq_advantage() {
        let checker = checker_with_freqs(&[("cache", 50_000_000)]);
        // "cash" is similarly frequent and distance-2 → low confidence
        let conf = checker.compute_confidence("cache", 60_000_000, 2);
        assert!(
            conf < 0.7,
            "expected low confidence for distance-2 similar-freq, got {conf}"
        );
    }

    #[test]
    fn confidence_unknown_original_word_gets_high_score() {
        let checker = checker_with_freqs(&[]);
        // Unknown word (freq=0 → clamped to 1) → strong freq advantage
        let conf = checker.compute_confidence("wrold", 5_000_000_000, 1);
        assert!(
            conf > 0.8,
            "expected high confidence for unknown→common, got {conf}"
        );
    }

    // ── Custom word protection tests ──

    #[test]
    fn custom_words_are_never_corrected() {
        let mut checker = SpellChecker::new_empty();
        checker
            .base_word_freqs
            .insert("cash".to_string(), 50_000_000);
        checker.word_freqs = checker.base_word_freqs.clone();
        checker.symspell = SpellChecker::build_symspell_from_freqs(&checker.word_freqs);

        // Add "cache" as a custom word
        checker.update_custom_words(&["cache".to_string()]);

        // "cache" should produce no edits since it's a custom word
        let edits = checker.process("use the cache");
        let cache_edits: Vec<_> = edits.iter().filter(|e| e.replacement == "cash").collect();
        assert!(
            cache_edits.is_empty(),
            "custom word 'cache' should not be corrected"
        );
    }

    #[test]
    fn custom_dictionary_phrase_is_tokenized_safely() {
        let mut checker = SpellChecker::new_empty();
        checker.update_custom_words(&["new york".to_string(), "O'Brien".to_string()]);

        assert!(checker.custom_words.contains("new"));
        assert!(checker.custom_words.contains("york"));
        assert!(checker.custom_words.contains("o'brien"));
    }

    #[test]
    fn custom_dictionary_invalid_tokens_are_ignored() {
        let mut checker = SpellChecker::new_empty();
        checker.update_custom_words(&["c++".to_string(), "rust-2026".to_string()]);

        assert!(!checker.custom_words.contains("c++"));
        assert!(!checker.custom_words.contains("rust-2026"));
    }

    #[test]
    fn symspell_build_handles_whitespace_terms_without_panic() {
        let mut entries = HashMap::new();
        entries.insert("new york".to_string(), 1);
        entries.insert("safe".to_string(), 42);
        let _ = SpellChecker::build_symspell_from_freqs(&entries);
    }

    #[test]
    fn does_not_drop_apostrophes_from_contractions() {
        let mut checker = SpellChecker::new_empty();
        checker
            .base_word_freqs
            .insert("dont".to_string(), 20_000_000);
        checker
            .base_word_freqs
            .insert("do".to_string(), 500_000_000);
        checker
            .base_word_freqs
            .insert("that".to_string(), 500_000_000);
        checker.word_freqs = checker.base_word_freqs.clone();
        checker.symspell = SpellChecker::build_symspell_from_freqs(&checker.word_freqs);

        let edits = checker.process("don't do that");
        assert!(edits.is_empty());
    }

    #[test]
    fn apostrophe_drop_guard_targets_contractions_only() {
        assert!(drops_apostrophe_from_contraction("don't", "dont"));
        assert!(drops_apostrophe_from_contraction("we're", "were"));
        assert!(!drops_apostrophe_from_contraction("o'brien", "obrien"));
        assert!(!drops_apostrophe_from_contraction("dont", "dont"));
    }
}
