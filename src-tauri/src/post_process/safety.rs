use strsim::levenshtein;

use super::TextEdit;

/// Safety gate that filters edits to prevent meaning drift.
pub struct SafetyGate {
    /// Maximum Levenshtein distance ratio (edit_distance / original_length).
    pub max_edit_distance_ratio: f32,
    /// Minimum confidence threshold for auto-apply.
    pub min_confidence: f32,
    /// Maximum percentage of total text that can be changed.
    pub max_total_change_ratio: f32,
}

impl Default for SafetyGate {
    fn default() -> Self {
        Self {
            max_edit_distance_ratio: 0.3,
            min_confidence: 0.7,
            max_total_change_ratio: 0.4,
        }
    }
}

impl SafetyGate {
    /// Filter a set of edits, returning (accepted, rejected) edits.
    pub fn filter_edits(&self, edits: &[TextEdit], text: &str) -> (Vec<TextEdit>, Vec<TextEdit>) {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let text_len = text.len();
        let mut total_changed_bytes: usize = 0;

        for edit in edits {
            let Some(end) = edit.offset.checked_add(edit.length) else {
                rejected.push(edit.clone());
                continue;
            };
            if end > text.len()
                || !text.is_char_boundary(edit.offset)
                || !text.is_char_boundary(end)
            {
                rejected.push(edit.clone());
                continue;
            }

            // Check confidence threshold
            if edit.confidence < self.min_confidence {
                rejected.push(edit.clone());
                continue;
            }

            // Check edit distance ratio for replacements (not insertions/deletions)
            if edit.length > 0 {
                let original = &text[edit.offset..end];

                // Skip the distance ratio check for pure case changes (e.g. "h" → "H",
                // "google" → "Google").  These are safe and should not be blocked by
                // the edit-distance heuristic which is designed for spelling corrections.
                if !is_case_only_change(original, &edit.replacement) {
                    let distance = levenshtein(original, &edit.replacement);
                    let ratio = distance as f32 / original.len().max(1) as f32;

                    if ratio > self.max_edit_distance_ratio {
                        rejected.push(edit.clone());
                        continue;
                    }
                }
            }

            // Check total change ratio
            let change_bytes = if edit.length > edit.replacement.len() {
                edit.length - edit.replacement.len()
            } else {
                edit.replacement.len() - edit.length
            };
            let prospective_total = total_changed_bytes
                .saturating_add(change_bytes)
                .saturating_add(edit.length);

            if text_len > 0
                && (prospective_total as f32 / text_len as f32) > self.max_total_change_ratio
            {
                rejected.push(edit.clone());
                continue;
            }

            total_changed_bytes = prospective_total;
            accepted.push(edit.clone());
        }

        (accepted, rejected)
    }
}

/// Returns true when `original` and `replacement` differ only in letter case.
fn is_case_only_change(original: &str, replacement: &str) -> bool {
    original.len() == replacement.len()
        && original
            .chars()
            .zip(replacement.chars())
            .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post_process::PipelineStage;

    fn make_edit(offset: usize, length: usize, replacement: &str, confidence: f32) -> TextEdit {
        TextEdit {
            offset,
            length,
            replacement: replacement.to_string(),
            source: PipelineStage::SpellCorrection,
            confidence,
            rule_id: "test".to_string(),
        }
    }

    #[test]
    fn accepts_high_confidence_small_edits() {
        let gate = SafetyGate::default();
        // "recieve" → "receive" has edit distance 2 over 7 chars = 0.28 < 0.3 threshold
        let text = "I will recieve the package in the mail";
        let edits = vec![make_edit(7, 7, "receive", 0.9)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 1);
        assert_eq!(rejected.len(), 0);
    }

    #[test]
    fn rejects_low_confidence_edits() {
        let gate = SafetyGate::default();
        let text = "hello wrold";
        let edits = vec![make_edit(6, 5, "world", 0.3)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn rejects_high_distance_ratio_edits() {
        let gate = SafetyGate::default();
        let text = "cat";
        // Replacing "cat" with "completely" is a very high distance ratio
        let edits = vec![make_edit(0, 3, "completely", 0.9)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn accepts_insertions() {
        let gate = SafetyGate::default();
        let text = "hello world";
        let edits = vec![make_edit(5, 0, ",", 0.85)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 1);
        assert_eq!(rejected.len(), 0);
    }

    #[test]
    fn rejects_when_total_change_too_large() {
        let gate = SafetyGate {
            max_total_change_ratio: 0.1,
            ..SafetyGate::default()
        };
        let text = "hi";
        // Changing the entire short text
        let edits = vec![make_edit(0, 2, "hello world", 0.9)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 1);
    }

    // ── Case-only change tests ──

    #[test]
    fn accepts_single_char_case_change() {
        let gate = SafetyGate::default();
        // "h" → "H" has distance ratio 1.0, but is a pure case change
        // so the distance ratio check should be skipped.
        let text = "hello world.";
        let edits = vec![make_edit(0, 1, "H", 0.95)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 1, "case-only change should be accepted");
        assert_eq!(rejected.len(), 0);
    }

    #[test]
    fn accepts_multi_char_case_change() {
        let gate = SafetyGate::default();
        // "i" → "I" (standalone pronoun)
        let text = "then i went home.";
        let edits = vec![make_edit(5, 1, "I", 0.85)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 1, "case-only change should be accepted");
        assert_eq!(rejected.len(), 0);
    }

    #[test]
    fn still_rejects_non_case_high_ratio_edits() {
        let gate = SafetyGate::default();
        // "cat" → "dog" is NOT a case-only change and has ratio 1.0
        let text = "cat";
        let edits = vec![make_edit(0, 3, "dog", 0.9)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn is_case_only_change_works() {
        assert!(is_case_only_change("h", "H"));
        assert!(is_case_only_change("hello", "Hello"));
        assert!(is_case_only_change("HELLO", "hello"));
        assert!(is_case_only_change("google", "Google"));
        assert!(!is_case_only_change("cat", "dog"));
        assert!(!is_case_only_change("h", "ha"));
        assert!(!is_case_only_change("abc", "ab"));
    }

    #[test]
    fn rejects_invalid_edit_span_out_of_bounds() {
        let gate = SafetyGate::default();
        let text = "hello";
        let edits = vec![make_edit(10, 1, "x", 0.9)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert!(accepted.is_empty());
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn rejects_invalid_edit_span_non_char_boundary() {
        let gate = SafetyGate::default();
        let text = "cafe\u{301}";
        let edits = vec![make_edit(5, 1, "x", 0.9)];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert!(accepted.is_empty());
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn rejected_large_edit_does_not_consume_change_budget() {
        let gate = SafetyGate {
            max_total_change_ratio: 0.3,
            ..SafetyGate::default()
        };
        let text = "abcdefghij";
        let edits = vec![
            // 4/10 change ratio => rejected
            make_edit(0, 0, "zzzz", 0.9),
            // 1/10 change ratio => should still be accepted
            make_edit(5, 0, "x", 0.9),
        ];
        let (accepted, rejected) = gate.filter_edits(&edits, text);
        assert_eq!(accepted.len(), 1);
        assert_eq!(rejected.len(), 1);
        assert_eq!(accepted[0].replacement, "x");
    }
}
