use std::path::Path;

use buttervoice_core::capture::{AudioFormat, CapturedAudio, InputKind};
use buttervoice_core::language::Language;
use buttervoice_samples::{Kind, RecordedWith, Sample, SampleSet, SuiteError, load_suite, save};

fn a_set_by(speaker: &str) -> SampleSet {
    SampleSet {
        recorded_with: RecordedWith {
            speaker: speaker.to_string(),
            app_version: "0.2.0".to_string(),
            app_commit: "0a1b2c3d4e5f".to_string(),
            chip: "Apple M5 Pro".to_string(),
            macos: "27.2".to_string(),
        },
        samples: vec![Sample {
            kind: Kind::ShortUtterance,
            language: Language::English,
            said: "Yes.".to_string(),
            prompt: Some("Yes.".to_string()),
            conditions: None,
            input: InputKind::BuiltIn,
            audio: CapturedAudio {
                format: AudioFormat {
                    sample_rate: 16_000,
                    channels: 1,
                },
                samples: vec![0.25, -0.5],
                dropped_samples: 0,
            },
        }],
    }
}

fn names(suite: &Path) -> Vec<String> {
    load_suite(suite)
        .unwrap()
        .into_iter()
        .map(|set| set.name)
        .collect()
}

#[test]
fn every_set_in_a_suite_is_loaded_in_name_order_and_nothing_else_is() {
    let suite = tempfile::tempdir().unwrap();
    save(&suite.path().join("speaker-b"), &a_set_by("b")).unwrap();
    save(&suite.path().join("speaker-a"), &a_set_by("a")).unwrap();
    std::fs::write(suite.path().join("notes.txt"), "not a set").unwrap();
    std::fs::create_dir(suite.path().join("scratch")).unwrap();

    let loaded = load_suite(suite.path()).unwrap();

    assert_eq!(
        loaded
            .iter()
            .map(|set| set.name.as_str())
            .collect::<Vec<_>>(),
        ["speaker-a", "speaker-b"]
    );
    assert_eq!(loaded[0].set, a_set_by("a"));
}

#[test]
fn a_set_folder_given_on_its_own_is_a_suite_of_one() {
    let suite = tempfile::tempdir().unwrap();
    let set = suite.path().join("speaker-a");
    save(&set, &a_set_by("a")).unwrap();

    assert_eq!(names(&set), ["speaker-a"]);
}

#[test]
fn a_set_that_cannot_be_read_stops_the_suite_and_is_named() {
    let suite = tempfile::tempdir().unwrap();
    save(&suite.path().join("speaker-a"), &a_set_by("a")).unwrap();
    let broken = suite.path().join("broken");
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join("set.json"), "{").unwrap();

    let Err(SuiteError::Set { name, .. }) = load_suite(suite.path()) else {
        panic!("a broken set should stop the suite");
    };
    assert_eq!(name, "broken");
}

#[test]
fn a_folder_with_no_sets_in_it_is_refused() {
    let suite = tempfile::tempdir().unwrap();
    std::fs::create_dir(suite.path().join("scratch")).unwrap();

    assert!(matches!(load_suite(suite.path()), Err(SuiteError::NoSets)));
}
