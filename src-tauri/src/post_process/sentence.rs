use super::{PipelineStage, TextEdit};

/// Common abbreviations that should not trigger sentence boundaries.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "ave", "blvd", "dept", "est", "govt", "inc",
    "corp", "ltd", "co", "vs", "etc", "approx", "dept", "div", "fig", "gen", "gov", "hon", "misc",
    "no", "vol", "rev", "sgt", "cpl", "pvt", "lt", "capt", "maj", "col", "cmdr", "adm", "jan",
    "feb", "mar", "apr", "jun", "jul", "aug", "sep", "oct", "nov", "dec", "mon", "tue", "wed",
    "thu", "fri", "sat", "sun", "a.m", "p.m", "u.s", "u.k", "e.g", "i.e", "al",
];

pub struct SentenceSegmenter {
    abbreviations: Vec<String>,
}

impl Default for SentenceSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl SentenceSegmenter {
    pub fn new() -> Self {
        Self {
            abbreviations: ABBREVIATIONS.iter().map(|s| s.to_lowercase()).collect(),
        }
    }

    /// Process text: ensure sentences are properly separated.
    /// Returns edits where sentence boundaries need fixing.
    pub fn process(&self, text: &str) -> Vec<TextEdit> {
        let mut edits = Vec::new();
        let bytes = text.as_bytes();
        let len = bytes.len();

        let mut i = 0;
        while i < len {
            let b = bytes[i];

            // Look for sentence-ending punctuation followed by a letter without space
            if (b == b'.' || b == b'!' || b == b'?') && i + 1 < len {
                // Check if this period is part of an abbreviation
                if b == b'.' && self.is_abbreviation(text, i) {
                    i += 1;
                    continue;
                }

                // Check if next char is a letter with no space
                let next = bytes[i + 1];
                if next.is_ascii_alphabetic() {
                    // Insert a space after the punctuation
                    edits.push(TextEdit {
                        offset: i + 1,
                        length: 0,
                        replacement: " ".to_string(),
                        source: PipelineStage::SentenceSegmentation,
                        confidence: 0.85,
                        rule_id: "sentence_boundary_space".to_string(),
                    });
                }
            }

            i += 1;
        }

        edits
    }

    /// Check if a period at `dot_pos` is part of an abbreviation.
    fn is_abbreviation(&self, text: &str, dot_pos: usize) -> bool {
        // Walk backwards from the dot to find the word
        let before = &text[..dot_pos];
        let word_start = before
            .rfind(|c: char| !c.is_alphabetic() && c != '.')
            .map(|p| p + 1)
            .unwrap_or(0);
        let word = &text[word_start..dot_pos].to_lowercase();

        if word.is_empty() {
            return false;
        }

        // Check against abbreviation list
        if self.abbreviations.contains(word) {
            return true;
        }

        // Single letter followed by dot (e.g., "U." in "U.S.")
        if word.len() == 1 && word.chars().next().is_some_and(|c| c.is_alphabetic()) {
            return true;
        }

        // Check for dotted abbreviation pattern (e.g., "U.S.A.")
        if word.contains('.') {
            return true;
        }

        // Decimal numbers: check if the character before the word is a digit
        if dot_pos + 1 < text.len() {
            let after = text.as_bytes()[dot_pos + 1];
            if word.chars().all(|c| c.is_ascii_digit()) && after.is_ascii_digit() {
                return true;
            }
        }
        if word.chars().all(|c| c.is_ascii_digit()) {
            // Could be a number like "3.14" — check what comes after the dot
            if dot_pos + 1 < text.len() && text.as_bytes()[dot_pos + 1].is_ascii_digit() {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_changes_for_normal_text() {
        let seg = SentenceSegmenter::new();
        let edits = seg.process("Hello world. This is fine.");
        assert!(edits.is_empty());
    }

    #[test]
    fn inserts_space_after_sentence_boundary() {
        let seg = SentenceSegmenter::new();
        let edits = seg.process("Hello world.This is fine.");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].offset, 12);
        assert_eq!(edits[0].replacement, " ");
    }

    #[test]
    fn preserves_abbreviations() {
        let seg = SentenceSegmenter::new();
        let edits = seg.process("Dr. Smith went home.");
        assert!(edits.is_empty());
    }

    #[test]
    fn preserves_decimal_numbers() {
        let seg = SentenceSegmenter::new();
        let edits = seg.process("The value is 3.14 percent.");
        assert!(edits.is_empty());
    }

    #[test]
    fn handles_exclamation_and_question() {
        let seg = SentenceSegmenter::new();
        let edits = seg.process("Wow!Great job?Really.");
        // "!" before "G" and "?" before "R" need spaces; "." at end has no following char
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn preserves_us_abbreviation() {
        let seg = SentenceSegmenter::new();
        let edits = seg.process("The U.S. is large.");
        assert!(edits.is_empty());
    }
}
