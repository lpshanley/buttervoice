use buttervoice_lib::post_process::metrics::{char_error_rate, percentile, word_error_rate};
use buttervoice_lib::post_process::PostProcessor;
use buttervoice_lib::settings::Settings;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BenchmarkCase {
    name: String,
    raw_whisper: String,
    reference: String,
    #[allow(dead_code)]
    domain: String,
}

#[derive(Debug)]
struct CaseResult {
    name: String,
    wer_before: f64,
    wer_after: f64,
    #[allow(dead_code)]
    cer_before: f64,
    cer_after: f64,
    duration_ms: u64,
    stage_wers: Vec<(String, f64)>,
}

fn load_benchmark_cases() -> Vec<BenchmarkCase> {
    let raw = include_str!("fixtures/post_process_benchmark.json");
    serde_json::from_str(raw).expect("benchmark fixture json should parse")
}

fn default_settings() -> Settings {
    Settings {
        post_process_enabled: true,
        post_process_spell_enabled: true,
        post_process_itn_enabled: true,
        post_process_grammar_rules_enabled: true,
        ..Settings::default()
    }
}

#[test]
fn benchmark_post_processing_quality() {
    let cases = load_benchmark_cases();
    let processor = PostProcessor::new_fallback();
    let settings = default_settings();

    let mut results: Vec<CaseResult> = Vec::new();

    for case in &cases {
        // WER/CER before pipeline
        let wer_before_result = word_error_rate(&case.reference, &case.raw_whisper);
        let cer_before = char_error_rate(&case.reference, &case.raw_whisper);

        // Run pipeline with intermediates
        let (pipeline_result, intermediates) = processor
            .run_with_intermediates(&case.raw_whisper, &settings)
            .expect("pipeline should run");

        // WER/CER after pipeline
        let wer_after_result = word_error_rate(&case.reference, &pipeline_result.output.as_str());
        let cer_after = char_error_rate(&case.reference, &pipeline_result.output.as_str());

        // Per-stage WER tracking
        let mut stage_wers: Vec<(String, f64)> = Vec::new();
        for (stage, text) in &intermediates {
            let stage_wer = word_error_rate(&case.reference, text);
            stage_wers.push((stage.label().to_string(), stage_wer.wer));
        }

        results.push(CaseResult {
            name: case.name.clone(),
            wer_before: wer_before_result.wer,
            wer_after: wer_after_result.wer,
            cer_before,
            cer_after,
            duration_ms: pipeline_result.total_duration_ms,
            stage_wers,
        });
    }

    // ── Print summary table ──
    eprintln!();
    eprintln!(
        "{:<35} | {:>10} | {:>9} | {:>7} | {:>9} | {:>8}",
        "Case", "WER Before", "WER After", "Delta", "CER After", "Duration"
    );
    eprintln!("{}", "-".repeat(90));

    for r in &results {
        let delta = r.wer_after - r.wer_before;
        eprintln!(
            "{:<35} | {:>9.1}% | {:>8.1}% | {:>+6.1}% | {:>8.1}% | {:>5}ms",
            r.name,
            r.wer_before * 100.0,
            r.wer_after * 100.0,
            delta * 100.0,
            r.cer_after * 100.0,
            r.duration_ms,
        );
    }

    // ── Averages ──
    let n = results.len() as f64;
    let avg_wer_before = results.iter().map(|r| r.wer_before).sum::<f64>() / n;
    let avg_wer_after = results.iter().map(|r| r.wer_after).sum::<f64>() / n;
    let avg_cer_after = results.iter().map(|r| r.cer_after).sum::<f64>() / n;
    let avg_delta = avg_wer_after - avg_wer_before;

    eprintln!("{}", "-".repeat(90));
    eprintln!(
        "{:<35} | {:>9.1}% | {:>8.1}% | {:>+6.1}% | {:>8.1}% |",
        "AVERAGE",
        avg_wer_before * 100.0,
        avg_wer_after * 100.0,
        avg_delta * 100.0,
        avg_cer_after * 100.0,
    );

    // ── Perfect transcript rate ──
    let perfect_count = results.iter().filter(|r| r.wer_after == 0.0).count();
    eprintln!();
    eprintln!(
        "Perfect transcript rate: {}/{} ({:.1}%)",
        perfect_count,
        results.len(),
        (perfect_count as f64 / n) * 100.0
    );

    // ── Per-stage contribution table ──
    // Collect all unique stage names in order
    let stage_order: Vec<String> = if let Some(first) = results.first() {
        first.stage_wers.iter().map(|(s, _)| s.clone()).collect()
    } else {
        Vec::new()
    };

    if !stage_order.is_empty() {
        eprintln!();
        eprintln!(
            "{:<25} | {:>13} | {:>20}",
            "Stage", "Avg WER After", "Avg Delta from Prev"
        );
        eprintln!("{}", "-".repeat(65));

        eprintln!(
            "{:<25} | {:>12.1}% | {:>19}",
            "raw_input",
            avg_wer_before * 100.0,
            "—"
        );

        let mut prev_avg = avg_wer_before;
        for stage_name in &stage_order {
            let stage_wers: Vec<f64> = results
                .iter()
                .filter_map(|r| {
                    r.stage_wers
                        .iter()
                        .find(|(s, _)| s == stage_name)
                        .map(|(_, w)| *w)
                })
                .collect();

            if stage_wers.is_empty() {
                continue;
            }

            let avg = stage_wers.iter().sum::<f64>() / stage_wers.len() as f64;
            let delta = avg - prev_avg;
            eprintln!(
                "{:<25} | {:>12.1}% | {:>+18.1}%",
                stage_name,
                avg * 100.0,
                delta * 100.0
            );
            prev_avg = avg;
        }
    }

    // ── Latency percentiles ──
    let mut durations: Vec<u64> = results.iter().map(|r| r.duration_ms).collect();
    let p50 = percentile(&mut durations.clone(), 0.5);
    let p95 = percentile(&mut durations.clone(), 0.95);
    let p99 = percentile(&mut durations, 0.99);
    eprintln!();
    eprintln!("Latency: P50={}ms  P95={}ms  P99={}ms", p50, p95, p99);
    eprintln!();

    // ── Assertions ──

    // Pipeline should not make things worse overall
    assert!(
        avg_wer_after <= avg_wer_before + 0.001, // small epsilon for floating point
        "Pipeline made average WER worse: {:.1}% before → {:.1}% after",
        avg_wer_before * 100.0,
        avg_wer_after * 100.0
    );

    // No individual case should regress by more than 10% WER
    for r in &results {
        let regression = r.wer_after - r.wer_before;
        assert!(
            regression <= 0.10 + 1e-9,
            "Case '{}' regressed by {:.1}% WER (before: {:.1}%, after: {:.1}%)",
            r.name,
            regression * 100.0,
            r.wer_before * 100.0,
            r.wer_after * 100.0,
        );
    }
}
