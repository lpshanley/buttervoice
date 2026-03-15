use super::{PipelineStage, TextEdit};

/// Common proper nouns and words that should always be capitalized.
const PROPER_NOUNS: &[&str] = &[
    // Tech
    "Google",
    "Apple",
    "Microsoft",
    "Amazon",
    "Facebook",
    "Meta",
    "Twitter",
    "Netflix",
    "Spotify",
    "GitHub",
    "Linux",
    "Windows",
    "Android",
    "iPhone",
    "iPad",
    "MacBook",
    "Chrome",
    "Firefox",
    "Safari",
    "JavaScript",
    "TypeScript",
    "Python",
    "Rust",
    "React",
    "Angular",
    "Vue",
    "Node",
    "Docker",
    "Kubernetes",
    // Countries
    "America",
    "American",
    "Canada",
    "Canadian",
    "Mexico",
    "Mexican",
    "England",
    "English",
    "Britain",
    "British",
    "France",
    "French",
    "Germany",
    "German",
    "Italy",
    "Italian",
    "Spain",
    "Spanish",
    "China",
    "Chinese",
    "Japan",
    "Japanese",
    "Korea",
    "Korean",
    "India",
    "Indian",
    "Australia",
    "Australian",
    "Brazil",
    "Brazilian",
    "Russia",
    "Russian",
    "Europe",
    "European",
    "Africa",
    "African",
    "Asia",
    "Asian",
    // Days and months
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
    // Pronoun
    "I",
];

pub struct Truecaser {
    /// Lowercase lookup → proper form
    proper_lookup: std::collections::HashMap<String, String>,
}

impl Default for Truecaser {
    fn default() -> Self {
        Self::new()
    }
}

impl Truecaser {
    pub fn new() -> Self {
        let proper_lookup: std::collections::HashMap<String, String> = PROPER_NOUNS
            .iter()
            .map(|s| (s.to_lowercase(), s.to_string()))
            .collect();

        Self { proper_lookup }
    }

    /// Process text: capitalize sentence starts and fix known proper nouns.
    pub fn process(&self, text: &str) -> Vec<TextEdit> {
        let mut edits = Vec::new();

        self.capitalize_sentence_starts(text, &mut edits);
        self.fix_proper_nouns(text, &mut edits);

        edits
    }

    /// Capitalize the first letter of each sentence.
    fn capitalize_sentence_starts(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let bytes = text.as_bytes();
        let len = bytes.len();

        // Find the first alphabetic character in the text
        if let Some((first_pos, first_char)) = text.char_indices().find(|(_, c)| c.is_alphabetic())
        {
            if first_char.is_lowercase() {
                let upper: String = first_char.to_uppercase().collect();
                edits.push(TextEdit {
                    offset: first_pos,
                    length: first_char.len_utf8(),
                    replacement: upper,
                    source: PipelineStage::Truecasing,
                    confidence: 0.95,
                    rule_id: "capitalize_sentence_start".to_string(),
                });
            }
        }

        // Find sentence boundaries and capitalize the next letter
        let mut i = 0;
        while i < len {
            let b = bytes[i];
            if matches!(b, b'.' | b'!' | b'?') {
                // Skip any whitespace after the punctuation
                let mut j = i + 1;
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                // Check if the next character is a lowercase letter
                if j < len {
                    if let Some(ch) = text[j..].chars().next() {
                        if ch.is_lowercase() {
                            let upper: String = ch.to_uppercase().collect();
                            edits.push(TextEdit {
                                offset: j,
                                length: ch.len_utf8(),
                                replacement: upper,
                                source: PipelineStage::Truecasing,
                                confidence: 0.95,
                                rule_id: "capitalize_sentence_start".to_string(),
                            });
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// Fix known proper nouns to their correct capitalization.
    fn fix_proper_nouns(&self, text: &str, edits: &mut Vec<TextEdit>) {
        // Split text into words and check each
        let mut word_start = None;

        for (i, ch) in text.char_indices() {
            if ch.is_alphanumeric() || ch == '\'' {
                if word_start.is_none() {
                    word_start = Some(i);
                }
            } else if let Some(start) = word_start {
                let word = &text[start..i];
                self.check_proper_noun(word, start, edits);
                word_start = None;
            }
        }

        // Handle the last word
        if let Some(start) = word_start {
            let word = &text[start..];
            self.check_proper_noun(word, start, edits);
        }
    }

    fn check_proper_noun(&self, word: &str, offset: usize, edits: &mut Vec<TextEdit>) {
        let lower = word.to_lowercase();

        if let Some(proper) = self.proper_lookup.get(&lower) {
            // Don't fix if it's already correct
            if word != proper.as_str() {
                // Don't create a duplicate edit if there's already one at this offset
                // (from capitalize_sentence_starts)
                if edits.iter().any(|e| e.offset == offset) {
                    return;
                }

                // Special case: "I" — only fix standalone "i", not "i" in other words
                if proper == "I" && word.len() != 1 {
                    return;
                }

                edits.push(TextEdit {
                    offset,
                    length: word.len(),
                    replacement: proper.clone(),
                    source: PipelineStage::Truecasing,
                    confidence: 0.85,
                    rule_id: format!("proper_noun_{}", lower),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post_process::apply_edits;

    #[test]
    fn capitalizes_sentence_start() {
        let tc = Truecaser::new();
        let edits = tc.process("hello world.");
        let result = apply_edits("hello world.", &edits);
        assert_eq!(result, "Hello world.");
    }

    #[test]
    fn capitalizes_after_sentence_boundary() {
        let tc = Truecaser::new();
        let edits = tc.process("Hello world. this is fine.");
        let result = apply_edits("Hello world. this is fine.", &edits);
        assert_eq!(result, "Hello world. This is fine.");
    }

    #[test]
    fn fixes_proper_nouns() {
        let tc = Truecaser::new();
        let edits = tc.process("I use google chrome on linux.");
        let result = apply_edits("I use google chrome on linux.", &edits);
        assert_eq!(result, "I use Google Chrome on Linux.");
    }

    #[test]
    fn fixes_standalone_i() {
        let tc = Truecaser::new();
        let edits = tc.process("then i went home.");
        let result = apply_edits("then i went home.", &edits);
        assert!(result.contains(" I "));
    }

    #[test]
    fn no_changes_for_correct_text() {
        let tc = Truecaser::new();
        let edits = tc.process("Hello world. This is fine.");
        assert!(edits.is_empty());
    }

    #[test]
    fn fixes_day_names() {
        let tc = Truecaser::new();
        let edits = tc.process("Meeting on monday.");
        let result = apply_edits("Meeting on monday.", &edits);
        assert_eq!(result, "Meeting on Monday.");
    }
}
