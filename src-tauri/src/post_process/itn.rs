use regex::Regex;
use std::collections::HashMap;

use super::{PipelineStage, TextEdit};

/// Inverse Text Normalization: convert spoken forms to written forms.
/// e.g., "twenty five dollars" → "$25", "three thirty pm" → "3:30 PM"
pub struct InverseTextNormalizer {
    cardinal_map: HashMap<&'static str, u64>,
    ordinal_map: HashMap<&'static str, &'static str>,
    multipliers: HashMap<&'static str, u64>,
    currency_patterns: Vec<CurrencyPattern>,
    _time_pattern: Regex,
}

struct CurrencyPattern {
    word: &'static str,
    symbol: &'static str,
    prefix: bool,
}

impl Default for InverseTextNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl InverseTextNormalizer {
    pub fn new() -> Self {
        let mut cardinal_map = HashMap::new();
        let singles = [
            ("zero", 0),
            ("one", 1),
            ("two", 2),
            ("three", 3),
            ("four", 4),
            ("five", 5),
            ("six", 6),
            ("seven", 7),
            ("eight", 8),
            ("nine", 9),
            ("ten", 10),
            ("eleven", 11),
            ("twelve", 12),
            ("thirteen", 13),
            ("fourteen", 14),
            ("fifteen", 15),
            ("sixteen", 16),
            ("seventeen", 17),
            ("eighteen", 18),
            ("nineteen", 19),
            ("twenty", 20),
            ("thirty", 30),
            ("forty", 40),
            ("fifty", 50),
            ("sixty", 60),
            ("seventy", 70),
            ("eighty", 80),
            ("ninety", 90),
        ];
        for (word, val) in singles {
            cardinal_map.insert(word, val);
        }

        let mut multipliers = HashMap::new();
        multipliers.insert("hundred", 100_u64);
        multipliers.insert("thousand", 1_000);
        multipliers.insert("million", 1_000_000);
        multipliers.insert("billion", 1_000_000_000);

        let mut ordinal_map = HashMap::new();
        let ordinals = [
            ("first", "1st"),
            ("second", "2nd"),
            ("third", "3rd"),
            ("fourth", "4th"),
            ("fifth", "5th"),
            ("sixth", "6th"),
            ("seventh", "7th"),
            ("eighth", "8th"),
            ("ninth", "9th"),
            ("tenth", "10th"),
            ("eleventh", "11th"),
            ("twelfth", "12th"),
            ("thirteenth", "13th"),
            ("fourteenth", "14th"),
            ("fifteenth", "15th"),
            ("sixteenth", "16th"),
            ("seventeenth", "17th"),
            ("eighteenth", "18th"),
            ("nineteenth", "19th"),
            ("twentieth", "20th"),
            ("thirtieth", "30th"),
            ("fortieth", "40th"),
            ("fiftieth", "50th"),
        ];
        for (word, val) in ordinals {
            ordinal_map.insert(word, val);
        }

        let currency_patterns = vec![
            CurrencyPattern {
                word: "dollars",
                symbol: "$",
                prefix: true,
            },
            CurrencyPattern {
                word: "dollar",
                symbol: "$",
                prefix: true,
            },
            CurrencyPattern {
                word: "euros",
                symbol: "\u{20AC}",
                prefix: true,
            },
            CurrencyPattern {
                word: "euro",
                symbol: "\u{20AC}",
                prefix: true,
            },
            CurrencyPattern {
                word: "pounds",
                symbol: "\u{00A3}",
                prefix: true,
            },
            CurrencyPattern {
                word: "pound",
                symbol: "\u{00A3}",
                prefix: true,
            },
            CurrencyPattern {
                word: "cents",
                symbol: "\u{00A2}",
                prefix: false,
            },
            CurrencyPattern {
                word: "percent",
                symbol: "%",
                prefix: false,
            },
        ];

        // Pattern for time: "NUMBER COLON_WORD NUMBER am/pm"
        let _time_pattern =
            Regex::new(r"(?i)\b(\w+)\s+(o'clock|oclock)\s*(am|pm|a\.m\.|p\.m\.)?").unwrap();

        Self {
            cardinal_map,
            ordinal_map,
            multipliers,
            currency_patterns,
            _time_pattern,
        }
    }

    pub fn process(&self, text: &str) -> Vec<TextEdit> {
        let mut edits = Vec::new();

        self.convert_currency(text, &mut edits);
        self.convert_ordinals(text, &mut edits);
        self.convert_standalone_numbers(text, &mut edits);

        // Sort by offset and remove overlapping edits (keep earlier ones)
        edits.sort_by_key(|e| e.offset);
        let mut filtered = Vec::new();
        let mut last_end = 0;
        for edit in edits {
            if edit.offset >= last_end {
                last_end = edit.offset + edit.length;
                filtered.push(edit);
            }
        }

        filtered
    }

    /// Convert "NUMBER dollars/euros/etc" → "$NUMBER"
    fn convert_currency(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let words: Vec<(usize, &str)> = self.word_spans(text);

        for i in 0..words.len() {
            let (_, word) = words[i];
            let lower = word.to_lowercase();

            for cp in &self.currency_patterns {
                if lower == cp.word {
                    // Look backwards to find the number
                    if i > 0 {
                        let num_result = self.parse_number_words_before(&words, i);
                        if let Some((num_start_idx, value)) = num_result {
                            let span_start = words[num_start_idx].0;
                            let span_end = words[i].0 + words[i].1.len();

                            let formatted = if cp.prefix {
                                format!("{}{}", cp.symbol, value)
                            } else {
                                format!("{}{}", value, cp.symbol)
                            };

                            edits.push(TextEdit {
                                offset: span_start,
                                length: span_end - span_start,
                                replacement: formatted,
                                source: PipelineStage::InverseTextNorm,
                                confidence: 0.85,
                                rule_id: format!("itn_currency_{}", cp.word),
                            });
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Convert ordinal words to numeric ordinals.
    fn convert_ordinals(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let words: Vec<(usize, &str)> = self.word_spans(text);

        for (offset, word) in &words {
            let lower = word.to_lowercase();
            if let Some(ordinal) = self.ordinal_map.get(lower.as_str()) {
                edits.push(TextEdit {
                    offset: *offset,
                    length: word.len(),
                    replacement: ordinal.to_string(),
                    source: PipelineStage::InverseTextNorm,
                    confidence: 0.8,
                    rule_id: "itn_ordinal".to_string(),
                });
            }
        }
    }

    /// Convert standalone number words to digits when they appear as a sequence.
    /// "twenty five" → "25", "one hundred" → "100"
    fn convert_standalone_numbers(&self, text: &str, edits: &mut Vec<TextEdit>) {
        let words: Vec<(usize, &str)> = self.word_spans(text);

        let mut i = 0;
        while i < words.len() {
            let lower = words[i].1.to_lowercase();

            // Check if this word starts a number sequence
            if self.cardinal_map.contains_key(lower.as_str())
                || self.multipliers.contains_key(lower.as_str())
            {
                // Try to parse a multi-word number starting here
                let (end_idx, value) = self.parse_number_words_forward(&words, i);

                if end_idx > i {
                    let span_start = words[i].0;
                    let span_end = words[end_idx].0 + words[end_idx].1.len();
                    let original = &text[span_start..span_end];

                    // Only convert if it's more than one word or a known single number
                    let word_count = end_idx - i + 1;
                    if word_count >= 2 || (word_count == 1 && value >= 10) {
                        let formatted = value.to_string();
                        if formatted != original {
                            edits.push(TextEdit {
                                offset: span_start,
                                length: span_end - span_start,
                                replacement: formatted,
                                source: PipelineStage::InverseTextNorm,
                                confidence: 0.8,
                                rule_id: "itn_number".to_string(),
                            });
                        }
                    }

                    i = end_idx + 1;
                    continue;
                }
            }

            i += 1;
        }
    }

    /// Parse number words going forward from index `start`.
    /// Returns (last_index_consumed, value).
    fn parse_number_words_forward(&self, words: &[(usize, &str)], start: usize) -> (usize, u64) {
        let mut total: u64 = 0;
        let mut current: u64 = 0;
        let mut last_valid = start;
        let mut found_any = false;

        let mut i = start;
        while i < words.len() {
            let lower = words[i].1.to_lowercase();

            if let Some(&val) = self.cardinal_map.get(lower.as_str()) {
                current += val;
                last_valid = i;
                found_any = true;
            } else if let Some(&mult) = self.multipliers.get(lower.as_str()) {
                if current == 0 {
                    current = 1;
                }
                if mult >= 1000 {
                    total += current * mult;
                    current = 0;
                } else {
                    current *= mult;
                }
                last_valid = i;
                found_any = true;
            } else if lower == "and" && found_any && i + 1 < words.len() {
                // "one hundred and twenty" — skip "and" if followed by more numbers
                let next_lower = words[i + 1].1.to_lowercase();
                if self.cardinal_map.contains_key(next_lower.as_str())
                    || self.multipliers.contains_key(next_lower.as_str())
                {
                    i += 1;
                    continue;
                } else {
                    break;
                }
            } else {
                break;
            }

            i += 1;
        }

        if !found_any {
            return (start, 0);
        }

        total += current;
        (last_valid, total)
    }

    /// Look backwards from `currency_idx` to find number words.
    fn parse_number_words_before(
        &self,
        words: &[(usize, &str)],
        currency_idx: usize,
    ) -> Option<(usize, u64)> {
        // Walk backwards to find the start of the number
        let mut start = currency_idx;
        let mut i = currency_idx.saturating_sub(1);

        loop {
            let lower = words[i].1.to_lowercase();
            if self.cardinal_map.contains_key(lower.as_str())
                || self.multipliers.contains_key(lower.as_str())
                || lower == "and"
            {
                start = i;
            } else {
                break;
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }

        if start >= currency_idx {
            return None;
        }

        let (end, value) = self.parse_number_words_forward(words, start);
        if value > 0 && end < currency_idx {
            Some((start, value))
        } else {
            None
        }
    }

    /// Split text into word spans: (byte_offset, word_str).
    fn word_spans<'a>(&self, text: &'a str) -> Vec<(usize, &'a str)> {
        let mut spans = Vec::new();
        let mut in_word = false;
        let mut word_start = 0;

        for (i, ch) in text.char_indices() {
            if ch.is_alphanumeric() || ch == '\'' || ch == '.' {
                if !in_word {
                    word_start = i;
                    in_word = true;
                }
            } else if in_word {
                let word = &text[word_start..i];
                // Trim trailing punctuation from the word
                let trimmed = word.trim_end_matches(['.', '\'']);
                if !trimmed.is_empty() {
                    spans.push((word_start, trimmed));
                }
                in_word = false;
            }
        }

        if in_word {
            let word = &text[word_start..];
            let trimmed = word.trim_end_matches(['.', '\'']);
            if !trimmed.is_empty() {
                spans.push((word_start, trimmed));
            }
        }

        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post_process::apply_edits;

    #[test]
    fn converts_simple_number() {
        let itn = InverseTextNormalizer::new();
        let edits = itn.process("I have twenty five apples.");
        let result = apply_edits("I have twenty five apples.", &edits);
        assert_eq!(result, "I have 25 apples.");
    }

    #[test]
    fn converts_currency() {
        let itn = InverseTextNormalizer::new();
        let edits = itn.process("It costs twenty five dollars.");
        let result = apply_edits("It costs twenty five dollars.", &edits);
        assert_eq!(result, "It costs $25.");
    }

    #[test]
    fn converts_ordinals() {
        let itn = InverseTextNormalizer::new();
        let edits = itn.process("The first item.");
        let result = apply_edits("The first item.", &edits);
        assert_eq!(result, "The 1st item.");
    }

    #[test]
    fn converts_hundred() {
        let itn = InverseTextNormalizer::new();
        let edits = itn.process("About one hundred people.");
        let result = apply_edits("About one hundred people.", &edits);
        assert_eq!(result, "About 100 people.");
    }

    #[test]
    fn converts_percent() {
        let itn = InverseTextNormalizer::new();
        let edits = itn.process("About fifty percent done.");
        let result = apply_edits("About fifty percent done.", &edits);
        assert_eq!(result, "About 50% done.");
    }

    #[test]
    fn no_conversion_for_small_standalone_numbers() {
        let itn = InverseTextNormalizer::new();
        // Single-word numbers under 10 should not be converted when standalone
        let edits = itn.process("I have five apples.");
        assert!(edits.is_empty());
    }

    #[test]
    fn converts_large_numbers() {
        let itn = InverseTextNormalizer::new();
        let edits = itn.process("About two thousand people.");
        let result = apply_edits("About two thousand people.", &edits);
        assert_eq!(result, "About 2000 people.");
    }
}
