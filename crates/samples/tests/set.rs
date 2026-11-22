use buttervoice_core::capture::{AudioFormat, CapturedAudio, InputKind};
use buttervoice_core::language::Language;
use buttervoice_samples::{Kind, LoadError, RecordedWith, Sample, SampleSet, load, save};

fn audio(samples: &[f32]) -> CapturedAudio {
    CapturedAudio {
        format: AudioFormat {
            sample_rate: 48_000,
            channels: 1,
        },
        samples: samples.to_vec(),
        dropped_samples: 0,
    }
}

fn a_set() -> SampleSet {
    SampleSet {
        recorded_with: RecordedWith {
            speaker: "speaker-a".to_string(),
            app_version: "0.2.0".to_string(),
            app_commit: "0a1b2c3d4e5f".to_string(),
            chip: "Apple M5 Pro".to_string(),
            macos: "27.2".to_string(),
        },
        samples: vec![
            Sample {
                kind: Kind::ReadSentence,
                language: Language::English,
                said: "That's everything for today.".to_string(),
                prompt: Some("That is everything for today.".to_string()),
                conditions: None,
                input: InputKind::BuiltIn,
                audio: audio(&[0.25, -0.5, 0.125]),
            },
            Sample {
                kind: Kind::Conditions,
                language: Language::BrazilianPortuguese,
                said: "Você pode me ouvir?".to_string(),
                prompt: None,
                conditions: Some("fan running".to_string()),
                input: InputKind::Other,
                audio: audio(&[-1.0, 1.0]),
            },
        ],
    }
}

#[test]
fn a_set_saved_to_a_folder_loads_back_exactly_as_it_was() {
    let folder = tempfile::tempdir().unwrap();

    save(folder.path(), &a_set()).unwrap();

    assert_eq!(load(folder.path()), Ok(a_set()));
}

#[test]
fn a_folder_that_holds_no_set_is_reported_as_such() {
    let folder = tempfile::tempdir().unwrap();

    assert_eq!(load(folder.path()), Err(LoadError::NotASet));
}

#[test]
fn a_set_from_a_version_this_one_cannot_read_is_refused_by_its_version() {
    let folder = tempfile::tempdir().unwrap();
    save(folder.path(), &a_set()).unwrap();
    let description = folder.path().join("set.json");
    let newer = std::fs::read_to_string(&description)
        .unwrap()
        .replace("\"format_version\":1", "\"format_version\":99");
    std::fs::write(&description, newer).unwrap();

    assert_eq!(load(folder.path()), Err(LoadError::UnknownVersion(99)));
}

#[test]
fn a_sample_whose_audio_has_gone_is_named() {
    let folder = tempfile::tempdir().unwrap();
    save(folder.path(), &a_set()).unwrap();
    std::fs::remove_file(folder.path().join("0002.wav")).unwrap();

    assert_eq!(
        load(folder.path()),
        Err(LoadError::MissingAudio("0002.wav".to_string()))
    );
}

#[test]
fn every_kind_of_sample_survives_being_saved() {
    let folder = tempfile::tempdir().unwrap();
    let one_of_each = SampleSet {
        samples: Kind::ALL
            .into_iter()
            .map(|kind| Sample {
                kind,
                ..a_set().samples[0].clone()
            })
            .collect(),
        ..a_set()
    };

    save(folder.path(), &one_of_each).unwrap();

    assert_eq!(load(folder.path()), Ok(one_of_each));
}
