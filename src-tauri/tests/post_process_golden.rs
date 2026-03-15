use buttervoice_lib::post_process::PostProcessor;
use buttervoice_lib::settings::Settings;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FixtureCase {
    name: String,
    input: String,
    expected_output: String,
    #[serde(default)]
    settings: FixtureSettings,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureSettings {
    post_process_spell_enabled: Option<bool>,
    post_process_itn_enabled: Option<bool>,
    post_process_grammar_rules_enabled: Option<bool>,
}

fn load_fixture() -> Vec<FixtureCase> {
    let raw = include_str!("fixtures/post_process_golden.json");
    serde_json::from_str(raw).expect("fixture json should parse")
}

fn case_settings(overrides: &FixtureSettings) -> Settings {
    let mut settings = Settings {
        post_process_enabled: true,
        ..Settings::default()
    };
    if let Some(v) = overrides.post_process_spell_enabled {
        settings.post_process_spell_enabled = v;
    }
    if let Some(v) = overrides.post_process_itn_enabled {
        settings.post_process_itn_enabled = v;
    }
    if let Some(v) = overrides.post_process_grammar_rules_enabled {
        settings.post_process_grammar_rules_enabled = v;
    }
    settings
}

#[test]
fn golden_outputs_match() {
    let processor = PostProcessor::new_fallback();
    for case in load_fixture() {
        let settings = case_settings(&case.settings);
        let result = processor
            .run(&case.input, &settings)
            .expect("pipeline should run");
        assert_eq!(
            result.output, case.expected_output,
            "golden mismatch for case '{}'",
            case.name
        );
    }
}

#[test]
fn deterministic_runs_are_identical() {
    let processor = PostProcessor::new_fallback();
    for case in load_fixture() {
        let settings = case_settings(&case.settings);
        let first = processor
            .run(&case.input, &settings)
            .expect("pipeline should run");
        let baseline_output = first.output.clone();
        let baseline_applied = serde_json::to_string(&first.applied_edits).unwrap();
        let baseline_rejected = serde_json::to_string(&first.rejected_edits).unwrap();
        for _ in 0..100 {
            let run = processor
                .run(&case.input, &settings)
                .expect("pipeline should run");
            assert_eq!(
                run.output, baseline_output,
                "output drift for '{}'",
                case.name
            );
            assert_eq!(
                serde_json::to_string(&run.applied_edits).unwrap(),
                baseline_applied,
                "applied edits drift for '{}'",
                case.name
            );
            assert_eq!(
                serde_json::to_string(&run.rejected_edits).unwrap(),
                baseline_rejected,
                "rejected edits drift for '{}'",
                case.name
            );
        }
    }
}

#[test]
fn idempotence_holds_for_golden_cases() {
    let processor = PostProcessor::new_fallback();
    for case in load_fixture() {
        let settings = case_settings(&case.settings);
        let once = processor
            .run(&case.input, &settings)
            .expect("pipeline should run");
        let twice = processor
            .run(&once.output, &settings)
            .expect("pipeline should run");
        assert_eq!(
            twice.output, once.output,
            "idempotence failed for '{}'",
            case.name
        );
    }
}
