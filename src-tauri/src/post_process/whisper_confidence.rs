use crate::whisper_backend::TokenConfidence;

/// Whisper probability above which corrections start being suppressed.
pub const DAMPEN_START_PROB: f32 = 0.60;
/// Maximum fraction of stage confidence removed at whisper prob 1.0 for
/// spell corrections.
pub const SPELL_MAX_SUPPRESSION: f32 = 0.35;

/// Whisper token confidence may only DAMPEN a stage's own confidence,
/// never raise it.
///
/// When the probability is unknown or at most [`DAMPEN_START_PROB`], the
/// stage confidence passes through untouched. As the probability rises
/// toward 1.0 the confidence is scaled down linearly by up to
/// `max_suppression` — the more certain whisper was about a token, the
/// less it should be second-guessed.
pub fn dampen_confidence(stage_conf: f32, whisper_prob: Option<f32>, max_suppression: f32) -> f32 {
    let Some(wp) = whisper_prob else {
        return stage_conf;
    };
    if wp <= DAMPEN_START_PROB {
        return stage_conf;
    }
    let t = ((wp - DAMPEN_START_PROB) / (1.0 - DAMPEN_START_PROB)).clamp(0.0, 1.0);
    (stage_conf * (1.0 - max_suppression * t)).clamp(0.0, 1.0)
}

/// Maps byte spans in post-processed text back to Whisper token probabilities.
///
/// Built after structural pipeline stages (sentence segmentation, punctuation,
/// truecasing, ITN) by aligning whisper tokens against the current text state.
pub struct WhisperConfidenceMap {
    /// (byte_offset, byte_end, probability) sorted by offset.
    spans: Vec<(usize, usize, f32)>,
}

impl WhisperConfidenceMap {
    /// Build the map by aligning whisper tokens against the current text.
    ///
    /// Walks words extracted from `tokens` and words from `text` in parallel,
    /// matching by lowercased form. This handles casing/punctuation changes
    /// from earlier pipeline stages that don't alter word identity.
    pub fn build(text: &str, tokens: &[TokenConfidence]) -> Self {
        let text_words = word_spans(text);
        let token_words = flatten_token_words(tokens);

        let mut spans = Vec::new();
        let mut tok_idx = 0;

        for (offset, end, text_word_lower) in &text_words {
            if tok_idx >= token_words.len() {
                break;
            }

            // Try to match the current text word to the next available token word.
            // Allow skipping up to 2 token words to handle minor alignment drift
            // (e.g. tokens that were filtered out or split differently).
            let mut matched = false;
            for lookahead in 0..3 {
                let candidate = tok_idx + lookahead;
                if candidate >= token_words.len() {
                    break;
                }
                if token_words[candidate].0 == *text_word_lower {
                    spans.push((*offset, *end, token_words[candidate].1));
                    tok_idx = candidate + 1;
                    matched = true;
                    break;
                }
            }

            if !matched {
                // No token match found — skip this text word (no confidence data).
                // Don't advance tok_idx; the token may match a later text word.
            }
        }

        Self { spans }
    }

    /// Average whisper probability for tokens overlapping `[offset, offset + length)`.
    ///
    /// Returns `None` if no token data covers this span.
    pub fn confidence_for_span(&self, offset: usize, length: usize) -> Option<f32> {
        let end = offset + length;
        let mut sum = 0.0_f32;
        let mut count = 0u32;

        for &(span_start, span_end, prob) in &self.spans {
            // Check overlap: spans overlap if start < other_end && other_start < end
            if span_start < end && offset < span_end {
                sum += prob;
                count += 1;
            }
        }

        if count > 0 {
            Some(sum / count as f32)
        } else {
            None
        }
    }
}

/// Extract word spans from text as (byte_offset, byte_end, lowercased_word).
fn word_spans(text: &str) -> Vec<(usize, usize, String)> {
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some(&(i, c)) = chars.peek() {
        if c.is_alphanumeric() || c == '\'' {
            let start = i;
            let mut end = i + c.len_utf8();
            chars.next();
            while let Some(&(j, c2)) = chars.peek() {
                if c2.is_alphanumeric() || c2 == '\'' {
                    end = j + c2.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &text[start..end];
            spans.push((start, end, word.to_lowercase()));
        } else {
            chars.next();
        }
    }

    spans
}

/// Flatten token list into (lowercased_word, probability) pairs.
///
/// Each token may contain leading/trailing whitespace or span multiple words
/// (rare). We split on whitespace and assign the token's probability to each word.
fn flatten_token_words(tokens: &[TokenConfidence]) -> Vec<(String, f32)> {
    let mut words = Vec::new();
    for tok in tokens {
        for part in tok.text.split_whitespace() {
            // Strip leading/trailing punctuation to match word extraction from text
            let trimmed: String = part
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .collect();
            if !trimmed.is_empty() {
                words.push((trimmed.to_lowercase(), tok.prob));
            }
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str, prob: f32) -> TokenConfidence {
        TokenConfidence {
            text: text.to_string(),
            prob,
        }
    }

    #[test]
    fn basic_alignment() {
        let tokens = vec![tok(" Hello", 0.95), tok(" world", 0.80)];
        let text = "Hello world";
        let map = WhisperConfidenceMap::build(text, &tokens);

        assert_eq!(map.spans.len(), 2);
        // "Hello" at [0, 5)
        let conf = map.confidence_for_span(0, 5).unwrap();
        assert!((conf - 0.95).abs() < 0.01);
        // "world" at [6, 11)
        let conf = map.confidence_for_span(6, 5).unwrap();
        assert!((conf - 0.80).abs() < 0.01);
    }

    #[test]
    fn case_insensitive_matching() {
        // Truecasing changed "hello" to "Hello" — should still match
        let tokens = vec![tok(" hello", 0.90), tok(" world", 0.70)];
        let text = "Hello World";
        let map = WhisperConfidenceMap::build(text, &tokens);

        assert_eq!(map.spans.len(), 2);
        let conf = map.confidence_for_span(0, 5).unwrap();
        assert!((conf - 0.90).abs() < 0.01);
    }

    #[test]
    fn no_match_returns_none() {
        let tokens = vec![tok(" hello", 0.90)];
        let text = "Goodbye";
        let map = WhisperConfidenceMap::build(text, &tokens);

        assert!(map.confidence_for_span(0, 7).is_none());
    }

    #[test]
    fn empty_tokens() {
        let map = WhisperConfidenceMap::build("Hello world", &[]);
        assert!(map.confidence_for_span(0, 5).is_none());
    }

    #[test]
    fn empty_text() {
        let tokens = vec![tok(" hello", 0.90)];
        let map = WhisperConfidenceMap::build("", &tokens);
        assert!(map.spans.is_empty());
    }

    #[test]
    fn punctuation_added_by_pipeline() {
        // Pipeline added a comma — words should still align
        let tokens = vec![tok(" Hello", 0.95), tok(" world", 0.80), tok(" foo", 0.60)];
        let text = "Hello, world. Foo";
        let map = WhisperConfidenceMap::build(text, &tokens);

        assert_eq!(map.spans.len(), 3);
        // "foo" / "Foo" should match despite case
        let conf = map.confidence_for_span(14, 3).unwrap();
        assert!((conf - 0.60).abs() < 0.01);
    }

    #[test]
    fn multi_word_span_averages() {
        let tokens = vec![tok(" hello", 0.90), tok(" world", 0.70)];
        let text = "hello world";
        let map = WhisperConfidenceMap::build(text, &tokens);

        // Span covering both words [0, 11)
        let conf = map.confidence_for_span(0, 11).unwrap();
        assert!((conf - 0.80).abs() < 0.01);
    }

    // ── dampen_confidence tests ──

    #[test]
    fn dampen_no_data_passthrough() {
        assert_eq!(dampen_confidence(0.85, None, SPELL_MAX_SUPPRESSION), 0.85);
    }

    #[test]
    fn dampen_low_wp_no_boost() {
        // Low whisper probability must leave the stage confidence exactly
        // as-is — never raise it.
        assert_eq!(
            dampen_confidence(0.72, Some(0.2), SPELL_MAX_SUPPRESSION),
            0.72
        );
        assert_eq!(
            dampen_confidence(0.72, Some(DAMPEN_START_PROB), SPELL_MAX_SUPPRESSION),
            0.72
        );
    }

    #[test]
    fn dampen_high_wp_suppresses() {
        // A strong ed1 correction (0.93) on a token whisper was very sure
        // about (0.95) must fall below the default 0.7 gate.
        let damped = dampen_confidence(0.93, Some(0.95), SPELL_MAX_SUPPRESSION);
        assert!(damped < 0.7, "expected < 0.7, got {damped}");
    }

    #[test]
    fn dampen_never_boosts_and_is_monotonic() {
        let mut prev = f32::MAX;
        for wp in [0.0, 0.3, 0.6, 0.7, 0.8, 0.9, 1.0] {
            let damped = dampen_confidence(0.9, Some(wp), SPELL_MAX_SUPPRESSION);
            assert!(damped <= 0.9, "boosted at wp={wp}: {damped}");
            assert!(damped <= prev, "not monotonic at wp={wp}");
            prev = damped;
        }
        // Full suppression amount at wp = 1.0.
        let floor = dampen_confidence(0.9, Some(1.0), SPELL_MAX_SUPPRESSION);
        assert!((floor - 0.9 * (1.0 - SPELL_MAX_SUPPRESSION)).abs() < 1e-6);
    }
}
