use buttervoice_core::capture::{AudioFormat, CapturedAudio};
use buttervoice_replay::recordings::{load_all, next_number, save};

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

#[test]
fn a_saved_recording_loads_back_exactly_as_it_was_captured() {
    let dir = tempfile::tempdir().unwrap();
    let captured = audio(&[0.25, -0.5, 0.001_234_5]);

    save(dir.path(), 1, &captured).unwrap();

    let loaded = load_all(dir.path()).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "001");
    assert_eq!(loaded[0].audio, captured);
}

#[test]
fn recordings_load_in_the_order_they_were_numbered() {
    let dir = tempfile::tempdir().unwrap();
    for number in [10, 2, 1] {
        save(dir.path(), number, &audio(&[0.5])).unwrap();
    }

    let names: Vec<String> = load_all(dir.path())
        .unwrap()
        .into_iter()
        .map(|recording| recording.name)
        .collect();

    assert_eq!(names, ["001", "002", "010"]);
}

#[test]
fn the_next_recording_is_numbered_after_the_ones_already_there() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(next_number(dir.path()), 1);

    save(dir.path(), 1, &audio(&[0.5])).unwrap();
    save(dir.path(), 2, &audio(&[0.5])).unwrap();

    assert_eq!(next_number(dir.path()), 3);
}

#[test]
fn a_folder_that_does_not_exist_yet_starts_at_one_and_is_created_on_save() {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("recordings");
    assert_eq!(next_number(&dir), 1);

    save(&dir, 1, &audio(&[0.5])).unwrap();

    assert_eq!(load_all(&dir).unwrap().len(), 1);
}
