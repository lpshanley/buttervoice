pub mod dictionary;
pub mod grammar_rules;
pub mod itn;
pub mod punctuation;
pub mod safety;
pub mod sentence;
pub mod spell;
pub mod truecase;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

use self::dictionary::DictionaryManager;
use self::grammar_rules::GrammarRules;
use self::itn::InverseTextNormalizer;
use self::punctuation::PunctuationRepairer;
use self::safety::SafetyGate;
use self::sentence::SentenceSegmenter;
use self::spell::SpellChecker;
use self::truecase::Truecaser;

// ── Core types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    /// Byte offset into the input text where the edit starts.
    pub offset: usize,
    /// Number of bytes in the original span being replaced.
    pub length: usize,
    /// The replacement text.
    pub replacement: String,
    /// Which pipeline stage produced this edit.
    pub source: PipelineStage,
    /// Confidence score 0.0..1.0 (1.0 = certain).
    pub confidence: f32,
    /// Human-readable rule ID or description.
    pub rule_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    SentenceSegmentation,
    Punctuation,
    Truecasing,
    InverseTextNorm,
    SpellCorrection,
    GrammarRules,
    GrammarGec,
}

impl PipelineStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::SentenceSegmentation => "sentence_segmentation",
            Self::Punctuation => "punctuation",
            Self::Truecasing => "truecasing",
            Self::InverseTextNorm => "itn",
            Self::SpellCorrection => "spell_correction",
            Self::GrammarRules => "grammar_rules",
            Self::GrammarGec => "grammar_gec",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    /// The original input text.
    pub input: String,
    /// The output text after applying accepted edits.
    pub output: String,
    /// All edits that were applied.
    pub applied_edits: Vec<TextEdit>,
    /// Edits that were rejected by safety gates.
    pub rejected_edits: Vec<TextEdit>,
    /// Time spent in each stage (stage label -> ms).
    pub stage_timings_ms: HashMap<String, u64>,
    /// Total pipeline duration in ms.
    pub total_duration_ms: u64,
}

#[derive(Default)]
struct StageAccumulator {
    applied: Vec<TextEdit>,
    rejected: Vec<TextEdit>,
    timings: HashMap<String, u64>,
}

// ── Pipeline orchestrator ──

pub struct PostProcessor {
    sentence_segmenter: SentenceSegmenter,
    punctuation_repairer: PunctuationRepairer,
    truecaser: Truecaser,
    itn: InverseTextNormalizer,
    spell_checker: SpellChecker,
    grammar_rules: GrammarRules,
    #[allow(dead_code)]
    safety_gate: SafetyGate,
}

impl PostProcessor {
    pub fn new(base_dir: &Path) -> Result<Self> {
        let dict_manager =
            DictionaryManager::new(base_dir).context("failed to initialize dictionary manager")?;

        let spell_checker =
            SpellChecker::new(&dict_manager).context("failed to initialize spell checker")?;
        let truecaser = Truecaser::new();

        Ok(Self {
            sentence_segmenter: SentenceSegmenter::new(),
            punctuation_repairer: PunctuationRepairer::new(),
            truecaser,
            itn: InverseTextNormalizer::new(),
            spell_checker,
            grammar_rules: GrammarRules::new(),
            safety_gate: SafetyGate::default(),
        })
    }

    /// Create a fallback PostProcessor that works without dictionary files.
    /// Spell checking will be a no-op.
    pub fn new_fallback() -> Self {
        Self {
            sentence_segmenter: SentenceSegmenter::new(),
            punctuation_repairer: PunctuationRepairer::new(),
            truecaser: Truecaser::new(),
            itn: InverseTextNormalizer::new(),
            spell_checker: SpellChecker::new_empty(),
            grammar_rules: GrammarRules::new(),
            safety_gate: SafetyGate::default(),
        }
    }

    /// Reload custom dictionary words (called when settings change).
    pub fn update_custom_dictionary(&mut self, words: &[String]) {
        self.spell_checker.update_custom_words(words);
    }

    /// Run the full pipeline on input text, respecting settings toggles.
    pub fn run(&self, text: &str, settings: &Settings) -> Result<PipelineResult> {
        let pipeline_start = Instant::now();
        let mut current_text = text.to_string();
        let mut stage_acc = StageAccumulator::default();

        // Update safety gate from settings
        let safety_gate = SafetyGate {
            min_confidence: settings.post_process_confidence_threshold,
            max_edit_distance_ratio: settings.post_process_max_edit_ratio,
            ..SafetyGate::default()
        };

        // Stage 1: Sentence segmentation (always runs — structural)
        current_text = self.run_stage(
            PipelineStage::SentenceSegmentation,
            &current_text,
            |text| self.sentence_segmenter.process(text),
            &safety_gate,
            &mut stage_acc,
        );

        // Stage 2: Punctuation repair (always runs — structural)
        current_text = self.run_stage(
            PipelineStage::Punctuation,
            &current_text,
            |text| self.punctuation_repairer.process(text),
            &safety_gate,
            &mut stage_acc,
        );

        // Stage 3: Truecasing (always runs — structural)
        current_text = self.run_stage(
            PipelineStage::Truecasing,
            &current_text,
            |text| self.truecaser.process(text),
            &safety_gate,
            &mut stage_acc,
        );

        // Stage 4: ITN (toggled)
        if settings.post_process_itn_enabled {
            current_text = self.run_stage(
                PipelineStage::InverseTextNorm,
                &current_text,
                |text| self.itn.process(text),
                &safety_gate,
                &mut stage_acc,
            );
        }

        // Stage 5: Spell correction (toggled)
        if settings.post_process_spell_enabled {
            current_text = self.run_stage(
                PipelineStage::SpellCorrection,
                &current_text,
                |text| self.spell_checker.process(text),
                &safety_gate,
                &mut stage_acc,
            );
        }

        // Stage 6: Grammar rules (toggled)
        if settings.post_process_grammar_rules_enabled {
            current_text = self.run_stage(
                PipelineStage::GrammarRules,
                &current_text,
                |text| self.grammar_rules.process(text),
                &safety_gate,
                &mut stage_acc,
            );
        }

        let total_duration_ms = pipeline_start.elapsed().as_millis() as u64;

        Ok(PipelineResult {
            input: text.to_string(),
            output: current_text.trim().to_string(),
            applied_edits: stage_acc.applied,
            rejected_edits: stage_acc.rejected,
            stage_timings_ms: stage_acc.timings,
            total_duration_ms,
        })
    }

    /// Run a single pipeline stage, apply safety gate, and return the resulting text.
    fn run_stage(
        &self,
        stage: PipelineStage,
        input: &str,
        process_fn: impl FnOnce(&str) -> Vec<TextEdit>,
        safety_gate: &SafetyGate,
        stage_acc: &mut StageAccumulator,
    ) -> String {
        let start = Instant::now();
        let edits = process_fn(input);
        let edits = canonicalize_edits(input, edits);
        let duration_ms = start.elapsed().as_millis() as u64;
        stage_acc
            .timings
            .insert(stage.label().to_string(), duration_ms);

        if edits.is_empty() {
            return input.to_string();
        }

        // Filter edits through safety gate
        let (safe_edits, unsafe_edits) = safety_gate.filter_edits(&edits, input);
        stage_acc.rejected.extend(unsafe_edits);

        if safe_edits.is_empty() {
            return input.to_string();
        }

        // Apply safe edits in reverse offset order to preserve positions
        let result = apply_edits(input, &safe_edits);
        stage_acc.applied.extend(safe_edits);
        result
    }
}

/// Apply a set of non-overlapping edits to text. Edits are sorted by offset descending
/// so that applying them doesn't shift earlier offsets.
pub fn apply_edits(text: &str, edits: &[TextEdit]) -> String {
    if edits.is_empty() {
        return text.to_string();
    }

    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by(|a, b| b.offset.cmp(&a.offset));

    let mut result = text.to_string();
    for edit in sorted {
        let Some(raw_end) = edit.offset.checked_add(edit.length) else {
            continue;
        };
        let end = raw_end.min(result.len());

        if edit.offset <= result.len()
            && result.is_char_boundary(edit.offset)
            && result.is_char_boundary(end)
        {
            result.replace_range(edit.offset..end, &edit.replacement);
        }
    }
    result
}

fn canonicalize_edits(input: &str, mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits.sort_by(|a, b| {
        a.offset
            .cmp(&b.offset)
            .then_with(|| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| a.length.cmp(&b.length))
            .then_with(|| a.source.label().cmp(b.source.label()))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
            .then_with(|| a.replacement.cmp(&b.replacement))
    });

    let mut normalized = Vec::new();
    let mut last_end = 0usize;

    for edit in edits {
        let Some(end) = edit.offset.checked_add(edit.length) else {
            continue;
        };
        if end > input.len() || !input.is_char_boundary(edit.offset) || !input.is_char_boundary(end)
        {
            continue;
        }

        // Deterministic overlap policy: keep first edit and drop later overlaps.
        if edit.offset < last_end {
            continue;
        }
        if edit.length == 0
            && normalized
                .last()
                .is_some_and(|prev: &TextEdit| prev.length == 0 && prev.offset == edit.offset)
        {
            continue;
        }

        last_end = end.max(last_end);
        normalized.push(edit);
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_edits_single_replacement() {
        let text = "hello wrold";
        let edits = vec![TextEdit {
            offset: 6,
            length: 5,
            replacement: "world".to_string(),
            source: PipelineStage::SpellCorrection,
            confidence: 0.95,
            rule_id: "spell_wrold".to_string(),
        }];
        assert_eq!(apply_edits(text, &edits), "hello world");
    }

    #[test]
    fn apply_edits_multiple_non_overlapping() {
        let text = "teh cat sat on teh mat";
        let edits = vec![
            TextEdit {
                offset: 0,
                length: 3,
                replacement: "the".to_string(),
                source: PipelineStage::SpellCorrection,
                confidence: 0.95,
                rule_id: "spell_teh".to_string(),
            },
            TextEdit {
                offset: 15,
                length: 3,
                replacement: "the".to_string(),
                source: PipelineStage::SpellCorrection,
                confidence: 0.95,
                rule_id: "spell_teh".to_string(),
            },
        ];
        assert_eq!(apply_edits(text, &edits), "the cat sat on the mat");
    }

    #[test]
    fn apply_edits_empty_returns_original() {
        let text = "hello world";
        assert_eq!(apply_edits(text, &[]), "hello world");
    }

    #[test]
    fn apply_edits_insertion() {
        let text = "hello world";
        let edits = vec![TextEdit {
            offset: 5,
            length: 0,
            replacement: ",".to_string(),
            source: PipelineStage::Punctuation,
            confidence: 0.8,
            rule_id: "comma_insert".to_string(),
        }];
        assert_eq!(apply_edits(text, &edits), "hello, world");
    }

    #[test]
    fn apply_edits_ignores_invalid_utf8_boundaries() {
        let text = "naive café";
        let edits = vec![TextEdit {
            offset: 10, // middle of "é" in UTF-8
            length: 1,
            replacement: "x".to_string(),
            source: PipelineStage::SpellCorrection,
            confidence: 0.95,
            rule_id: "bad_offset".to_string(),
        }];
        assert_eq!(apply_edits(text, &edits), text);
    }

    #[test]
    fn apply_edits_ignores_overflowing_span() {
        let text = "hello";
        let edits = vec![TextEdit {
            offset: usize::MAX,
            length: 10,
            replacement: "x".to_string(),
            source: PipelineStage::SpellCorrection,
            confidence: 0.95,
            rule_id: "overflow".to_string(),
        }];
        assert_eq!(apply_edits(text, &edits), text);
    }

    #[test]
    fn canonicalize_edits_orders_and_drops_overlaps() {
        let input = "hello world";
        let edits = vec![
            TextEdit {
                offset: 0,
                length: 5,
                replacement: "Hello".to_string(),
                source: PipelineStage::Truecasing,
                confidence: 0.8,
                rule_id: "case".to_string(),
            },
            TextEdit {
                offset: 0,
                length: 5,
                replacement: "hullo".to_string(),
                source: PipelineStage::SpellCorrection,
                confidence: 0.95,
                rule_id: "spell".to_string(),
            },
            TextEdit {
                offset: 3,
                length: 2,
                replacement: "XX".to_string(),
                source: PipelineStage::Punctuation,
                confidence: 1.0,
                rule_id: "overlap".to_string(),
            },
        ];
        let canonical = canonicalize_edits(input, edits);
        assert_eq!(canonical.len(), 1);
        assert_eq!(canonical[0].rule_id, "spell");
    }
}
