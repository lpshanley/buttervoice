use std::path::Path;

use buttervoice_core::capture::{AudioFormat, CapturedAudio, InputKind};
use buttervoice_core::language::Language;
use buttervoice_samples::session::{Prompt, Session, Take};
use buttervoice_samples::{Kind, RecordedWith, load};

fn read(words: &str) -> Prompt {
    Prompt {
        kind: Kind::ReadSentence,
        say: Some(words.to_string()),
        instruction: None,
        conditions: None,
    }
}

fn a_take(first_sample: f32) -> Take {
    Take {
        audio: CapturedAudio {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 1,
            },
            samples: vec![first_sample, 0.0],
            dropped_samples: 0,
        },
        input: InputKind::BuiltIn,
        heard: None,
    }
}

fn three_prompts() -> Session {
    let recorded_with = RecordedWith {
        speaker: "speaker-a".to_string(),
        app_version: "0.2.0".to_string(),
        app_commit: "0a1b2c3d4e5f".to_string(),
        chip: "Apple M5 Pro".to_string(),
        macos: "27.2".to_string(),
    };
    Session::new(
        recorded_with,
        Language::English,
        vec![read("one"), read("two"), read("three")],
    )
}

fn saids(folder: &Path) -> Vec<String> {
    load(folder)
        .unwrap()
        .samples
        .into_iter()
        .map(|sample| sample.said)
        .collect()
}

#[test]
fn each_kept_take_is_saved_under_its_prompt_so_skipping_renames_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let mut session = three_prompts();
    session.recorded(a_take(0.25));
    session.keep_into("one", folder.path()).unwrap();
    session.skip();
    session.recorded(a_take(0.5));
    session.keep_into("three", folder.path()).unwrap();

    assert!(folder.path().join("0001.wav").exists());
    assert!(!folder.path().join("0002.wav").exists());
    assert!(folder.path().join("0003.wav").exists());
    assert_eq!(saids(folder.path()), ["one", "three"]);
}

#[test]
fn recording_a_prompt_again_replaces_what_was_saved_for_it() {
    let folder = tempfile::tempdir().unwrap();
    let mut session = three_prompts();
    session.recorded(a_take(0.25));
    session.keep_into("one", folder.path()).unwrap();
    session.back();
    session.recorded(a_take(0.75));
    session.keep_into("one, again", folder.path()).unwrap();

    let set = load(folder.path()).unwrap();
    assert_eq!(set.samples.len(), 1);
    assert_eq!(set.samples[0].said, "one, again");
    assert_eq!(set.samples[0].audio.samples[0], 0.75);
}

#[test]
fn keeping_a_take_leaves_the_takes_saved_before_it_alone() {
    let folder = tempfile::tempdir().unwrap();
    let mut session = three_prompts();
    session.recorded(a_take(0.25));
    session.keep_into("one", folder.path()).unwrap();
    let first = folder.path().join("0001.wav");
    std::fs::write(&first, "as it was").unwrap();

    session.recorded(a_take(0.5));
    session.keep_into("two", folder.path()).unwrap();

    assert_eq!(std::fs::read_to_string(&first).unwrap(), "as it was");
}

#[test]
fn a_take_that_cannot_be_saved_is_still_waiting_and_nothing_moves_on() {
    let parent = tempfile::tempdir().unwrap();
    let not_a_folder = parent.path().join("set");
    std::fs::write(&not_a_folder, "a file where the folder should be").unwrap();
    let mut session = three_prompts();
    session.recorded(a_take(0.25));

    assert!(session.keep_into("one", &not_a_folder).is_err());

    assert_eq!(session.progress(), (0, 3));
    assert_eq!(session.kept_count(), 0);
    assert!(session.take().is_some());
}

#[test]
fn moving_on_without_a_take_saves_nothing_and_says_so() {
    let folder = tempfile::tempdir().unwrap();
    let mut session = three_prompts();

    assert!(!session.keep_into("one", folder.path()).unwrap());
    assert_eq!(session.progress(), (0, 3));
}

#[test]
fn saving_the_session_again_writes_whatever_the_folder_is_missing() {
    let folder = tempfile::tempdir().unwrap();
    let mut session = three_prompts();
    session.recorded(a_take(0.25));
    session.keep_into("one", folder.path()).unwrap();
    std::fs::remove_file(folder.path().join("0001.wav")).unwrap();

    session.save_into(folder.path()).unwrap();

    assert_eq!(saids(folder.path()), ["one"]);
}
