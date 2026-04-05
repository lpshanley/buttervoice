use buttervoice_lib::post_process::metrics::word_error_rate;
use buttervoice_lib::post_process::PostProcessor;
use buttervoice_lib::settings::Settings;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BenchmarkCase {
    name: String,
    raw_whisper: String,
    reference: String,
    domain: String,
}

fn load_spell_cases() -> Vec<BenchmarkCase> {
    let raw = include_str!("fixtures/post_process_benchmark.json");
    let all: Vec<BenchmarkCase> = serde_json::from_str(raw).expect("fixture json should parse");
    all.into_iter()
        .filter(|c| c.domain == "spelling")
        .collect()
}

fn spell_settings() -> Settings {
    Settings {
        post_process_enabled: true,
        post_process_spell_enabled: true,
        post_process_itn_enabled: false,
        post_process_grammar_rules_enabled: false,
        ..Settings::default()
    }
}

/// Benchmark spell correction quality using the real dictionary.
///
/// Run with: cargo test --test spell_benchmark -- --nocapture
#[test]
fn benchmark_spell_correction_quality() {
    let cases = load_spell_cases();
    assert!(!cases.is_empty(), "no spelling benchmark cases found");

    let temp_dir = std::env::temp_dir().join("buttervoice_spell_bench");
    let processor =
        PostProcessor::new(&temp_dir).expect("PostProcessor with dictionary should initialize");
    let settings = spell_settings();

    eprintln!();
    eprintln!(
        "{:<40} | {:>10} | {:>9} | {:>7} | {:>6} | {:>8}",
        "Case", "WER Before", "WER After", "Delta", "Edits", "Duration"
    );
    eprintln!("{}", "-".repeat(95));

    let mut total_wer_before = 0.0;
    let mut total_wer_after = 0.0;
    let mut total_edits_applied = 0u64;
    let mut total_edits_rejected = 0u64;

    for case in &cases {
        let wer_before = word_error_rate(&case.reference, &case.raw_whisper);

        let result = processor
            .run(&case.raw_whisper, &settings)
            .expect("pipeline should run");

        let wer_after = word_error_rate(&case.reference, &result.output.as_str());

        let applied = result.applied_edits.len() as u64;
        let rejected = result.rejected_edits.len() as u64;

        // Count spell-specific edits
        let spell_edits: Vec<_> = result
            .applied_edits
            .iter()
            .filter(|e| e.rule_id.starts_with("spell_ed"))
            .collect();

        let delta = wer_after.wer - wer_before.wer;
        eprintln!(
            "{:<40} | {:>9.1}% | {:>8.1}% | {:>+6.1}% | {:>6} | {:>5}ms",
            case.name,
            wer_before.wer * 100.0,
            wer_after.wer * 100.0,
            delta * 100.0,
            spell_edits.len(),
            result.total_duration_ms,
        );

        // Print individual spell corrections for analysis
        for edit in &spell_edits {
            let original = &case.raw_whisper[edit.offset..edit.offset + edit.length];
            eprintln!(
                "    {} → {} (conf={:.2}, {})",
                original, edit.replacement, edit.confidence, edit.rule_id
            );
        }

        total_wer_before += wer_before.wer;
        total_wer_after += wer_after.wer;
        total_edits_applied += applied;
        total_edits_rejected += rejected;
    }

    let n = cases.len() as f64;
    let avg_wer_before = total_wer_before / n;
    let avg_wer_after = total_wer_after / n;

    eprintln!("{}", "-".repeat(95));
    eprintln!(
        "{:<40} | {:>9.1}% | {:>8.1}% | {:>+6.1}% |",
        "AVERAGE",
        avg_wer_before * 100.0,
        avg_wer_after * 100.0,
        (avg_wer_after - avg_wer_before) * 100.0,
    );
    eprintln!();
    eprintln!(
        "Total edits: {} applied, {} rejected",
        total_edits_applied, total_edits_rejected
    );

    // Spell correction should not make things worse
    assert!(
        avg_wer_after <= avg_wer_before + 0.001,
        "Spell correction made average WER worse: {:.1}% → {:.1}%",
        avg_wer_before * 100.0,
        avg_wer_after * 100.0
    );
}
