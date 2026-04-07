use std::collections::{HashMap, HashSet};

use symspell::{AsciiStringStrategy, Suggestion, SymSpell, SymSpellBuilder, Verbosity};

use super::dictionary::DictionaryManager;
use super::whisper_confidence::WhisperConfidenceMap;
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
            .max_dictionary_edit_distance(3)
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
            3 => 0.6,
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

    /// Pick the best candidate from up to 3 suggestions using a blended score
    /// of edit distance, frequency, and length similarity.
    fn pick_best_candidate<'a>(
        &self,
        original: &str,
        suggestions: &'a [Suggestion],
    ) -> Option<&'a Suggestion> {
        let candidates = suggestions.iter().take(3);

        let orig_len = original.len() as f64;

        candidates
            .max_by(|a, b| {
                let score_a = self.candidate_score(original, orig_len, a);
                let score_b = self.candidate_score(original, orig_len, b);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Blended reranking score: 50% distance + 35% frequency + 15% length similarity.
    fn candidate_score(&self, original: &str, orig_len: f64, suggestion: &Suggestion) -> f64 {
        // Distance score: lower distance = higher score (normalized to [0, 1])
        let distance_score = 1.0 - (suggestion.distance as f64 / 3.0);

        // Frequency score: log-space ratio
        let original_freq = self.word_frequency(original).max(1) as f64;
        let suggestion_freq = suggestion.count.max(1) as f64;
        let freq_ratio = (suggestion_freq.ln() - original_freq.ln()) / 10.0_f64.ln();
        let freq_score = (freq_ratio.clamp(-1.0, 1.0) + 1.0) / 2.0;

        // Length similarity: penalize suggestions much shorter/longer than original
        let suggestion_len = suggestion.term.len() as f64;
        let len_diff = (orig_len - suggestion_len).abs();
        let length_score = 1.0 - (len_diff / orig_len.max(suggestion_len).max(1.0));

        0.50 * distance_score + 0.35 * freq_score + 0.15 * length_score
    }

    /// Process text: find and suggest corrections for misspelled words.
    pub fn process(&self, text: &str) -> Vec<TextEdit> {
        self.process_with_confidence(text, None)
    }

    /// Process text with optional whisper token confidence blending.
    ///
    /// When `whisper_conf` is provided, the final edit confidence blends the
    /// spell-checker's own confidence with whisper's uncertainty about the
    /// original token: `final = 0.55 * spell_conf + 0.45 * (1.0 - whisper_p)`.
    pub fn process_with_confidence(
        &self,
        text: &str,
        whisper_conf: Option<&WhisperConfidenceMap>,
    ) -> Vec<TextEdit> {
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

            // Fetch candidates at the closest edit distance (up to 3).
            let suggestions = self.symspell.lookup(&lower, Verbosity::Closest, 3);

            // Take top-3 candidates and pick the best by blended score.
            if let Some(best) = self.pick_best_candidate(&lower, &suggestions) {
                if best.distance > 0 && best.term != lower {
                    // Skip corrections where the edit distance covers the entire
                    // word — these are essentially replacing the whole word and
                    // are almost certainly wrong (e.g. "use" → "cash" at dist 3).
                    if best.distance as usize >= lower.len() {
                        continue;
                    }

                    // Do not degrade valid contractions into apostrophe-less
                    // single tokens, e.g. "don't" -> "dont" or
                    // "we're" -> "weare".
                    if degrades_contraction_to_single_token(&lower, &best.term) {
                        continue;
                    }

                    // Preserve original casing pattern
                    let replacement = match_case(word, &best.term);

                    // Combined confidence: edit distance + frequency evidence
                    let spell_conf =
                        self.compute_confidence(&lower, best.count, best.distance);

                    // Blend with whisper token confidence when available.
                    // Whisper's `p` = confidence the token is correct, so
                    // (1 - p) = uncertainty. High whisper confidence suppresses
                    // corrections; low whisper confidence permits them.
                    let confidence = match whisper_conf
                        .and_then(|wc| wc.confidence_for_span(*offset, word.len()))
                    {
                        Some(wp) => (0.55 * spell_conf + 0.45 * (1.0 - wp)).clamp(0.0, 1.0),
                        None => spell_conf,
                    };

                    edits.push(TextEdit {
                        offset: *offset,
                        length: word.len(),
                        replacement,
                        source: PipelineStage::SpellCorrection,
                        confidence,
                        rule_id: format!("spell_ed{}", best.distance),
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

fn degrades_contraction_to_single_token(original: &str, suggestion: &str) -> bool {
    if !original.contains('\'')
        || suggestion.contains('\'')
        || suggestion.chars().any(char::is_whitespace)
    {
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
                .max_dictionary_edit_distance(3)
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
    fn does_not_fuse_contractions_into_single_tokens() {
        let mut checker = SpellChecker::new_empty();
        checker
            .base_word_freqs
            .insert("weare".to_string(), 20_000_000);
        checker
            .base_word_freqs
            .insert("itis".to_string(), 20_000_000);
        checker
            .base_word_freqs
            .insert("going".to_string(), 500_000_000);
        checker
            .base_word_freqs
            .insert("working".to_string(), 500_000_000);
        checker.word_freqs = checker.base_word_freqs.clone();
        checker.symspell = SpellChecker::build_symspell_from_freqs(&checker.word_freqs);

        let edits = checker.process("we're going, it's working");
        assert!(edits.is_empty());
    }

    #[test]
    fn contraction_guard_targets_single_token_degradations_only() {
        assert!(degrades_contraction_to_single_token("don't", "dont"));
        assert!(degrades_contraction_to_single_token("we're", "were"));
        assert!(degrades_contraction_to_single_token("we're", "weare"));
        assert!(degrades_contraction_to_single_token("it's", "itis"));
        assert!(!degrades_contraction_to_single_token("we're", "we are"));
        assert!(!degrades_contraction_to_single_token("o'brien", "obrien"));
        assert!(!degrades_contraction_to_single_token("dont", "dont"));
    }

    // ── Distance-3 confidence tests ──

    #[test]
    fn confidence_distance3_with_strong_freq_advantage_passes_threshold() {
        let checker = checker_with_freqs(&[]);
        // Unknown word → very common suggestion at distance 3
        let conf = checker.compute_confidence("definately", 5_000_000_000, 3);
        // Should be >= 0.7 to pass safety gate
        assert!(
            conf >= 0.7,
            "distance-3 with strong freq advantage should pass 0.7 threshold, got {conf}"
        );
    }

    #[test]
    fn confidence_distance3_without_freq_advantage_below_threshold() {
        let checker = checker_with_freqs(&[("orignal", 50_000_000)]);
        // Similar frequency at distance 3 → should NOT pass safety gate
        let conf = checker.compute_confidence("orignal", 60_000_000, 3);
        assert!(
            conf < 0.7,
            "distance-3 without strong freq advantage should be below 0.7, got {conf}"
        );
    }

    // ── Top-k reranking tests ──

    #[test]
    fn pick_best_candidate_prefers_closer_distance() {
        let checker = checker_with_freqs(&[("test", 1_000_000)]);
        let suggestions = vec![
            Suggestion::new("far", 3, 5_000_000),
            Suggestion::new("close", 1, 5_000_000),
        ];
        let best = checker.pick_best_candidate("tset", &suggestions).unwrap();
        assert_eq!(best.term, "close");
    }

    #[test]
    fn pick_best_candidate_considers_frequency() {
        let checker = checker_with_freqs(&[("tset", 100)]);
        let suggestions = vec![
            Suggestion::new("rare", 1, 100),
            Suggestion::new("common", 1, 5_000_000_000),
        ];
        let best = checker.pick_best_candidate("tset", &suggestions).unwrap();
        assert_eq!(best.term, "common");
    }

    #[test]
    fn pick_best_candidate_considers_length_similarity() {
        let checker = checker_with_freqs(&[("wrod", 100)]);
        // Same distance and frequency, but "word" is same length as "wrod"
        let suggestions = vec![
            Suggestion::new("worldwide", 1, 5_000_000),
            Suggestion::new("word", 1, 5_000_000),
        ];
        let best = checker.pick_best_candidate("wrod", &suggestions).unwrap();
        assert_eq!(best.term, "word");
    }

    #[test]
    fn pick_best_candidate_returns_none_for_empty() {
        let checker = checker_with_freqs(&[]);
        let best = checker.pick_best_candidate("test", &[]);
        assert!(best.is_none());
    }
}
