use super::{PipelineStage, TextEdit};

pub struct PunctuationRepairer;

impl Default for PunctuationRepairer {
    fn default() -> Self {
        Self::new()
    }
}

impl PunctuationRepairer {
    pub fn new() -> Self {
        Self
    }

    /// Repair common punctuation issues in transcribed text.
    pub fn process(&self, text: &str) -> Vec<TextEdit> {
        let mut edits = Vec::new();

        self.fix_missing_final_period(text, &mut edits);
        self.fix_double_punctuation(text, &mut edits);
        self.fix_space_before_punctuation(text, &mut edits);
        self.fix_missing_space_after_comma(text, &mut edits);

        edits
    }

    /// Add period at end if text doesn't end with sentence-ending punctuation.
    fn fix_missing_final_period(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return;
        }

        let last_char = trimmed.chars().last().unwrap();
        if !matches!(
            last_char,
            '.' | '!' | '?' | ':' | ';' | '"' | '\'' | ')' | ']'
        ) {
            // Only add period if the text looks like a sentence (starts with uppercase or is long enough)
            let first_alpha = trimmed.chars().find(|c| c.is_alphabetic());
            let word_count = trimmed.split_whitespace().count();
            if word_count >= 3 || first_alpha.is_some_and(|c| c.is_uppercase()) {
                let insert_pos = trimmed.len();
                edits.push(TextEdit {
                    offset: insert_pos,
                    length: 0,
                    replacement: ".".to_string(),
                    source: PipelineStage::Punctuation,
                    confidence: 0.75,
                    rule_id: "missing_final_period".to_string(),
                });
            }
        }
    }

    /// Remove duplicate punctuation like ".." or "!!" (but keep "..." as ellipsis).
    fn fix_double_punctuation(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i + 1 < len {
            let curr = bytes[i];
            let next = bytes[i + 1];

            if is_sentence_punct(curr) && curr == next {
                // Check for ellipsis (three dots)
                if curr == b'.' && i + 2 < len && bytes[i + 2] == b'.' {
                    // Skip the ellipsis
                    i += 3;
                    continue;
                }

                // Remove the duplicate
                edits.push(TextEdit {
                    offset: i + 1,
                    length: 1,
                    replacement: String::new(),
                    source: PipelineStage::Punctuation,
                    confidence: 0.9,
                    rule_id: "double_punctuation".to_string(),
                });
                i += 2;
                continue;
            }

            i += 1;
        }
    }

    /// Remove spaces before punctuation: "hello ," → "hello,"
    fn fix_space_before_punctuation(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let bytes = text.as_bytes();
        let len = bytes.len();

        for i in 1..len {
            let curr = bytes[i];
            if matches!(curr, b',' | b'.' | b'!' | b'?' | b';' | b':') {
                // Check if preceded by a space
                if bytes[i - 1] == b' ' {
                    // Don't remove space before ellipsis start
                    if curr == b'.' && i + 2 < len && bytes[i + 1] == b'.' && bytes[i + 2] == b'.' {
                        continue;
                    }
                    edits.push(TextEdit {
                        offset: i - 1,
                        length: 1,
                        replacement: String::new(),
                        source: PipelineStage::Punctuation,
                        confidence: 0.9,
                        rule_id: "space_before_punctuation".to_string(),
                    });
                }
            }
        }
    }

    /// Add space after comma if missing: "hello,world" → "hello, world"
    fn fix_missing_space_after_comma(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let bytes = text.as_bytes();
        let len = bytes.len();

        for i in 0..len.saturating_sub(1) {
            let curr = bytes[i];
            let next = bytes[i + 1];

            if matches!(curr, b',' | b';' | b':') && next.is_ascii_alphabetic() {
                edits.push(TextEdit {
                    offset: i + 1,
                    length: 0,
                    replacement: " ".to_string(),
                    source: PipelineStage::Punctuation,
                    confidence: 0.85,
                    rule_id: "missing_space_after_punct".to_string(),
                });
            }
        }
    }
}

fn is_sentence_punct(b: u8) -> bool {
    matches!(b, b'.' | b'!' | b'?')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post_process::apply_edits;

    #[test]
    fn adds_final_period() {
        let pr = PunctuationRepairer::new();
        let edits = pr.process("Hello world this is a test");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].rule_id, "missing_final_period");
        let result = apply_edits("Hello world this is a test", &edits);
        assert_eq!(result, "Hello world this is a test.");
    }

    #[test]
    fn no_period_if_already_ends_with_punctuation() {
        let pr = PunctuationRepairer::new();
        let edits = pr.process("Hello world.");
        assert!(edits.is_empty());
    }

    #[test]
    fn removes_double_punctuation() {
        let pr = PunctuationRepairer::new();
        let edits = pr.process("Hello!!");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].rule_id, "double_punctuation");
    }

    #[test]
    fn preserves_ellipsis() {
        let pr = PunctuationRepairer::new();
        let edits = pr.process("Well...");
        assert!(edits.is_empty());
    }

    #[test]
    fn removes_space_before_comma() {
        let pr = PunctuationRepairer::new();
        let edits = pr.process("Hello , world.");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].rule_id, "space_before_punctuation");
        let result = apply_edits("Hello , world.", &edits);
        assert_eq!(result, "Hello, world.");
    }

    #[test]
    fn adds_space_after_comma() {
        let pr = PunctuationRepairer::new();
        let edits = pr.process("Hello,world.");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].rule_id, "missing_space_after_punct");
        let result = apply_edits("Hello,world.", &edits);
        assert_eq!(result, "Hello, world.");
    }

    #[test]
    fn no_changes_for_correct_text() {
        let pr = PunctuationRepairer::new();
        let edits = pr.process("Hello, world. This is fine!");
        assert!(edits.is_empty());
    }
}
