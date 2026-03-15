use regex::Regex;

use super::{PipelineStage, TextEdit};

struct GrammarRule {
    pattern: Regex,
    replacement: &'static str,
    confidence: f32,
    rule_id: &'static str,
}

pub struct GrammarRules {
    rules: Vec<GrammarRule>,
}

impl Default for GrammarRules {
    fn default() -> Self {
        Self::new()
    }
}

impl GrammarRules {
    pub fn new() -> Self {
        let rules = vec![
            // Article agreement: "a apple" → "an apple"
            GrammarRule {
                pattern: Regex::new(r"(?i)\ba\s+([aeiou]\w+)").unwrap(),
                replacement: "an $1",
                confidence: 0.9,
                rule_id: "article_an_before_vowel",
            },
            // Article agreement: "an cat" → "a cat"
            GrammarRule {
                pattern: Regex::new(r"(?i)\ban\s+([bcdfghjklmnpqrstvwxyz]\w+)").unwrap(),
                replacement: "a $1",
                confidence: 0.85,
                rule_id: "article_a_before_consonant",
            },
            // Subject-verb: "he don't" → "he doesn't"
            GrammarRule {
                pattern: Regex::new(r"(?i)\b(he|she|it)\s+don't\b").unwrap(),
                replacement: "$1 doesn't",
                confidence: 0.9,
                rule_id: "sv_agreement_dont",
            },
            // Subject-verb: "he have" → "he has"
            GrammarRule {
                pattern: Regex::new(r"(?i)\b(he|she|it)\s+have\b").unwrap(),
                replacement: "$1 has",
                confidence: 0.85,
                rule_id: "sv_agreement_have",
            },
            // Subject-verb: "they was" → "they were"
            GrammarRule {
                pattern: Regex::new(r"(?i)\b(they|we)\s+was\b").unwrap(),
                replacement: "$1 were",
                confidence: 0.85,
                rule_id: "sv_agreement_was",
            },
            // Subject-verb: "I is" → "I am"
            GrammarRule {
                pattern: Regex::new(r"\bI\s+is\b").unwrap(),
                replacement: "I am",
                confidence: 0.9,
                rule_id: "sv_agreement_i_is",
            },
            // Double negative: "can't not" → "can't"
            GrammarRule {
                pattern: Regex::new(r"(?i)\b(can't|cannot|won't|wouldn't|shouldn't|couldn't|didn't|doesn't|don't|isn't|aren't|wasn't|weren't)\s+not\b").unwrap(),
                replacement: "$1",
                confidence: 0.7,
                rule_id: "double_negative",
            },
            // "could of" → "could have"
            GrammarRule {
                pattern: Regex::new(
                    r"(?i)\b(could|would|should|might|must)\s+of\b",
                )
                .unwrap(),
                replacement: "$1 have",
                confidence: 0.9,
                rule_id: "modal_of_to_have",
            },
            // "alot" → "a lot"
            GrammarRule {
                pattern: Regex::new(r"(?i)\balot\b").unwrap(),
                replacement: "a lot",
                confidence: 0.95,
                rule_id: "alot_to_a_lot",
            },
            // "its" + verb → "it's" (possessive vs contraction)
            // Only for clear cases: "its a", "its the", "its been"
            GrammarRule {
                pattern: Regex::new(r"(?i)\bits\s+(a|an|the|been|not|going|been)\b").unwrap(),
                replacement: "it's $1",
                confidence: 0.8,
                rule_id: "its_to_its_contraction",
            },
            // "your" + verb → "you're" for clear cases
            GrammarRule {
                pattern: Regex::new(r"(?i)\byour\s+(welcome|right|wrong|going|not)\b").unwrap(),
                replacement: "you're $1",
                confidence: 0.8,
                rule_id: "your_to_youre",
            },
            // "their" + verb → "they're" for clear cases
            GrammarRule {
                pattern: Regex::new(r"(?i)\btheir\s+(going|not|coming|is|are|was|were)\b").unwrap(),
                replacement: "they're $1",
                confidence: 0.75,
                rule_id: "their_to_theyre",
            },
            // "then" in comparison → "than"
            GrammarRule {
                pattern: Regex::new(r"(?i)\b(more|less|better|worse|faster|slower|bigger|smaller|higher|lower|greater|fewer)\s+then\b").unwrap(),
                replacement: "$1 than",
                confidence: 0.85,
                rule_id: "then_to_than_comparison",
            },
            // "supposably" → "supposedly"
            GrammarRule {
                pattern: Regex::new(r"(?i)\bsupposably\b").unwrap(),
                replacement: "supposedly",
                confidence: 0.95,
                rule_id: "supposably_to_supposedly",
            },
            // "irregardless" → "regardless"
            GrammarRule {
                pattern: Regex::new(r"(?i)\birregardless\b").unwrap(),
                replacement: "regardless",
                confidence: 0.9,
                rule_id: "irregardless_to_regardless",
            },
        ];

        Self { rules }
    }

    /// Process text: apply grammar rules and return edits.
    pub fn process(&self, text: &str) -> Vec<TextEdit> {
        let mut edits = Vec::new();

        for rule in &self.rules {
            for mat in rule.pattern.find_iter(text) {
                // Check if this region overlaps with an existing edit
                let overlaps = edits.iter().any(|e: &TextEdit| {
                    e.offset < mat.end() && (e.offset + e.length) > mat.start()
                });
                if overlaps {
                    continue;
                }

                let matched_text = mat.as_str();
                let mut replacement = rule
                    .pattern
                    .replace(matched_text, rule.replacement)
                    .to_string();

                // Preserve leading case from original text
                if let (Some(orig_first), Some(repl_first)) =
                    (matched_text.chars().next(), replacement.chars().next())
                {
                    if orig_first.is_uppercase() && repl_first.is_lowercase() {
                        let mut chars = replacement.chars();
                        let upper: String = chars.next().unwrap().to_uppercase().collect();
                        replacement = upper + chars.as_str();
                    }
                }

                if replacement != matched_text {
                    edits.push(TextEdit {
                        offset: mat.start(),
                        length: mat.end() - mat.start(),
                        replacement: replacement.to_string(),
                        source: PipelineStage::GrammarRules,
                        confidence: rule.confidence,
                        rule_id: rule.rule_id.to_string(),
                    });
                }
            }
        }

        // Sort by offset
        edits.sort_by_key(|e| e.offset);
        edits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post_process::apply_edits;

    #[test]
    fn fixes_article_a_to_an() {
        let gr = GrammarRules::new();
        let edits = gr.process("I ate a apple.");
        let result = apply_edits("I ate a apple.", &edits);
        assert_eq!(result, "I ate an apple.");
    }

    #[test]
    fn fixes_could_of() {
        let gr = GrammarRules::new();
        let edits = gr.process("I could of done that.");
        let result = apply_edits("I could of done that.", &edits);
        assert_eq!(result, "I could have done that.");
    }

    #[test]
    fn fixes_subject_verb_agreement() {
        let gr = GrammarRules::new();
        let edits = gr.process("He don't like it.");
        let result = apply_edits("He don't like it.", &edits);
        assert_eq!(result, "He doesn't like it.");
    }

    #[test]
    fn fixes_alot() {
        let gr = GrammarRules::new();
        let edits = gr.process("I have alot of work.");
        let result = apply_edits("I have alot of work.", &edits);
        assert_eq!(result, "I have a lot of work.");
    }

    #[test]
    fn fixes_then_to_than() {
        let gr = GrammarRules::new();
        let edits = gr.process("This is better then that.");
        let result = apply_edits("This is better then that.", &edits);
        assert_eq!(result, "This is better than that.");
    }

    #[test]
    fn fixes_irregardless() {
        let gr = GrammarRules::new();
        let edits = gr.process("Irregardless of the outcome.");
        let result = apply_edits("Irregardless of the outcome.", &edits);
        assert_eq!(result, "Regardless of the outcome.");
    }

    #[test]
    fn no_changes_for_correct_text() {
        let gr = GrammarRules::new();
        let edits = gr.process("The cat sat on the mat.");
        assert!(edits.is_empty());
    }
}
