/// WER/CER computation for evaluating transcription and post-processing quality.

/// Normalize text for fair WER/CER comparison: lowercase, strip punctuation, collapse whitespace.
pub fn normalize(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_was_space = true; // trim leading whitespace

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                result.push(lower);
            }
            last_was_space = false;
        } else if ch.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        }
        // All other characters (punctuation, symbols) are stripped
    }

    // Trim trailing space
    if result.ends_with(' ') {
        result.pop();
    }

    result
}

#[derive(Debug, Clone)]
pub struct WerResult {
    /// Word Error Rate as a fraction (0.0 = perfect, 1.0+ = very bad).
    pub wer: f64,
    pub substitutions: usize,
    pub deletions: usize,
    pub insertions: usize,
    pub reference_length: usize,
}

/// Compute Word Error Rate between a reference and hypothesis.
/// Both strings are normalized before comparison.
pub fn word_error_rate(reference: &str, hypothesis: &str) -> WerResult {
    let ref_norm = normalize(reference);
    let hyp_norm = normalize(hypothesis);

    let ref_words: Vec<&str> = if ref_norm.is_empty() {
        Vec::new()
    } else {
        ref_norm.split_whitespace().collect()
    };
    let hyp_words: Vec<&str> = if hyp_norm.is_empty() {
        Vec::new()
    } else {
        hyp_norm.split_whitespace().collect()
    };

    let n = ref_words.len();
    let m = hyp_words.len();

    if n == 0 && m == 0 {
        return WerResult {
            wer: 0.0,
            substitutions: 0,
            deletions: 0,
            insertions: 0,
            reference_length: 0,
        };
    }
    if n == 0 {
        return WerResult {
            wer: 1.0,
            substitutions: 0,
            deletions: 0,
            insertions: m,
            reference_length: 0,
        };
    }

    // DP matrix for minimum edit distance on word sequences.
    // dp[i][j] = (cost, substitutions, deletions, insertions) to align ref[..i] with hyp[..j]
    let mut dp = vec![vec![(0usize, 0usize, 0usize, 0usize); m + 1]; n + 1];

    for i in 1..=n {
        dp[i][0] = (i, 0, i, 0); // delete all ref words
    }
    for j in 1..=m {
        dp[0][j] = (j, 0, 0, j); // insert all hyp words
    }

    for i in 1..=n {
        for j in 1..=m {
            let sub_cost = if ref_words[i - 1] == hyp_words[j - 1] {
                0
            } else {
                1
            };

            let substitute = (
                dp[i - 1][j - 1].0 + sub_cost,
                dp[i - 1][j - 1].1 + sub_cost,
                dp[i - 1][j - 1].2,
                dp[i - 1][j - 1].3,
            );
            let delete = (
                dp[i - 1][j].0 + 1,
                dp[i - 1][j].1,
                dp[i - 1][j].2 + 1,
                dp[i - 1][j].3,
            );
            let insert = (
                dp[i][j - 1].0 + 1,
                dp[i][j - 1].1,
                dp[i][j - 1].2,
                dp[i][j - 1].3 + 1,
            );

            dp[i][j] = if substitute.0 <= delete.0 && substitute.0 <= insert.0 {
                substitute
            } else if delete.0 <= insert.0 {
                delete
            } else {
                insert
            };
        }
    }

    let (_, subs, dels, ins) = dp[n][m];
    WerResult {
        wer: (subs + dels + ins) as f64 / n as f64,
        substitutions: subs,
        deletions: dels,
        insertions: ins,
        reference_length: n,
    }
}

/// Compute Character Error Rate between a reference and hypothesis.
/// Both strings are normalized before comparison.
/// Uses Levenshtein distance at the character level.
pub fn char_error_rate(reference: &str, hypothesis: &str) -> f64 {
    let ref_norm = normalize(reference);
    let hyp_norm = normalize(hypothesis);

    if ref_norm.is_empty() && hyp_norm.is_empty() {
        return 0.0;
    }
    if ref_norm.is_empty() {
        return 1.0;
    }

    let distance = strsim::levenshtein(&ref_norm, &hyp_norm);
    distance as f64 / ref_norm.chars().count() as f64
}

/// Compute a percentile from a mutable slice of values.
/// `p` should be between 0.0 and 1.0 (e.g., 0.5 for P50, 0.95 for P95).
pub fn percentile(values: &mut [u64], p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let idx = ((values.len() as f64 - 1.0) * p.clamp(0.0, 1.0)).round() as usize;
    values[idx.min(values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize ──

    #[test]
    fn normalize_lowercases_and_strips_punctuation() {
        assert_eq!(normalize("Hello, World!"), "hello world");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("  hello   world  "), "hello world");
    }

    #[test]
    fn normalize_strips_all_punctuation() {
        assert_eq!(
            normalize("The patient's temp. was 98.6°F."),
            "the patients temp was 986f"
        );
    }

    #[test]
    fn normalize_handles_empty() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("   "), "");
        assert_eq!(normalize("..."), "");
    }

    #[test]
    fn normalize_preserves_unicode_letters() {
        assert_eq!(normalize("Naïve café"), "naïve café");
    }

    // ── word_error_rate ──

    #[test]
    fn wer_identical() {
        let result = word_error_rate("hello world", "hello world");
        assert_eq!(result.wer, 0.0);
        assert_eq!(result.substitutions, 0);
        assert_eq!(result.deletions, 0);
        assert_eq!(result.insertions, 0);
    }

    #[test]
    fn wer_one_substitution() {
        // "the cat sat" vs "the dog sat" → 1 sub out of 3 words = 0.333...
        let result = word_error_rate("the cat sat", "the dog sat");
        assert!((result.wer - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(result.substitutions, 1);
        assert_eq!(result.deletions, 0);
        assert_eq!(result.insertions, 0);
    }

    #[test]
    fn wer_one_deletion() {
        // "the cat sat" vs "the sat" → 1 deletion out of 3 = 0.333...
        let result = word_error_rate("the cat sat", "the sat");
        assert!((result.wer - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(result.deletions, 1);
    }

    #[test]
    fn wer_one_insertion() {
        // "the sat" vs "the cat sat" → 1 insertion out of 2 = 0.5
        let result = word_error_rate("the sat", "the cat sat");
        assert!((result.wer - 0.5).abs() < 1e-9);
        assert_eq!(result.insertions, 1);
    }

    #[test]
    fn wer_ignores_case_and_punctuation() {
        let result = word_error_rate("Hello, World!", "hello world");
        assert_eq!(result.wer, 0.0);
    }

    #[test]
    fn wer_empty_both() {
        let result = word_error_rate("", "");
        assert_eq!(result.wer, 0.0);
    }

    #[test]
    fn wer_empty_reference_nonempty_hypothesis() {
        let result = word_error_rate("", "hello world");
        assert_eq!(result.wer, 1.0);
    }

    #[test]
    fn wer_nonempty_reference_empty_hypothesis() {
        let result = word_error_rate("hello world", "");
        assert!((result.wer - 1.0).abs() < 1e-9);
        assert_eq!(result.deletions, 2);
    }

    #[test]
    fn wer_completely_wrong() {
        // "a b c" vs "x y z" → 3 subs out of 3 = 1.0
        let result = word_error_rate("a b c", "x y z");
        assert!((result.wer - 1.0).abs() < 1e-9);
        assert_eq!(result.substitutions, 3);
    }

    // ── char_error_rate ──

    #[test]
    fn cer_identical() {
        assert_eq!(char_error_rate("hello", "hello"), 0.0);
    }

    #[test]
    fn cer_one_char_diff() {
        // "hello" (5 chars) vs "hallo" → 1 substitution / 5 = 0.2
        let cer = char_error_rate("hello", "hallo");
        assert!((cer - 0.2).abs() < 1e-9);
    }

    #[test]
    fn cer_ignores_punctuation_and_case() {
        assert_eq!(char_error_rate("Hello!", "hello"), 0.0);
    }

    #[test]
    fn cer_empty_both() {
        assert_eq!(char_error_rate("", ""), 0.0);
    }

    #[test]
    fn cer_empty_reference() {
        assert_eq!(char_error_rate("", "hello"), 1.0);
    }

    // ── percentile ──

    #[test]
    fn percentile_single_value() {
        assert_eq!(percentile(&mut [42], 0.5), 42);
    }

    #[test]
    fn percentile_p50() {
        let mut vals = vec![10, 20, 30, 40, 50];
        assert_eq!(percentile(&mut vals, 0.5), 30);
    }

    #[test]
    fn percentile_p95() {
        let mut vals: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&mut vals, 0.95), 95);
    }

    #[test]
    fn percentile_empty() {
        assert_eq!(percentile(&mut [], 0.5), 0);
    }
}
