use buttervoice_core::capture::{AudioFormat, CapturedAudio, InputKind};
use buttervoice_core::language::Language;
use buttervoice_samples::session::{Prompt, Session, Take};
use buttervoice_samples::{Kind, RecordedWith};

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
        heard: Some("what a model heard".to_string()),
    }
}

fn a_session(prompts: Vec<Prompt>) -> Session {
    let recorded_with = RecordedWith {
        speaker: "speaker-a".to_string(),
        app_version: "0.2.0".to_string(),
        app_commit: "0a1b2c3d4e5f".to_string(),
        chip: "Apple M5 Pro".to_string(),
        macos: "27.2".to_string(),
    };
    Session::new(recorded_with, Language::English, prompts)
}

#[test]
fn a_session_starts_on_its_first_prompt_with_nothing_recorded() {
    let session = a_session(vec![read("one"), read("two")]);

    assert_eq!(session.prompt(), Some(&read("one")));
    assert_eq!(session.take(), None);
    assert_eq!(session.progress(), (0, 2));
}

#[test]
fn a_kept_take_becomes_a_sample_of_what_the_person_says_they_said() {
    let quietly = Prompt {
        kind: Kind::Conditions,
        say: Some("That is everything for today.".to_string()),
        instruction: Some("Say it quietly.".to_string()),
        conditions: Some("quiet voice".to_string()),
    };
    let mut session = a_session(vec![quietly, read("two")]);
    session.recorded(a_take(0.25));

    assert!(session.keep("That's everything for today."));

    assert_eq!(session.prompt(), Some(&read("two")));
    assert_eq!(session.take(), None);
    assert_eq!(session.progress(), (1, 2));
    let set = session.finish();
    assert_eq!(set.recorded_with.speaker, "speaker-a");
    assert_eq!(set.samples.len(), 1);
    let sample = &set.samples[0];
    assert_eq!(sample.kind, Kind::Conditions);
    assert_eq!(sample.language, Language::English);
    assert_eq!(sample.said, "That's everything for today.");
    assert_eq!(
        sample.prompt.as_deref(),
        Some("That is everything for today.")
    );
    assert_eq!(sample.conditions.as_deref(), Some("quiet voice"));
    assert_eq!(sample.input, InputKind::BuiltIn);
    assert_eq!(sample.audio, a_take(0.25).audio);
}

#[test]
fn a_take_waits_to_be_kept_or_redone_and_does_not_move_the_session_on() {
    let mut session = a_session(vec![read("one"), read("two")]);

    session.recorded(a_take(0.25));

    assert_eq!(session.take(), Some(&a_take(0.25)));
    assert_eq!(session.prompt(), Some(&read("one")));
    assert_eq!(session.progress(), (0, 2));
}

#[test]
fn recording_again_before_keeping_replaces_the_take() {
    let mut session = a_session(vec![read("one")]);
    session.recorded(a_take(0.25));

    session.recorded(a_take(0.5));
    session.keep("one");

    assert_eq!(session.finish().samples[0].audio, a_take(0.5).audio);
}

#[test]
fn skipping_moves_on_without_a_sample_even_when_a_take_is_waiting() {
    let mut session = a_session(vec![read("one"), read("two")]);
    session.recorded(a_take(0.25));

    session.skip();

    assert_eq!(session.prompt(), Some(&read("two")));
    assert_eq!(session.take(), None);
    assert_eq!(session.progress(), (1, 2));
    assert!(session.finish().samples.is_empty());
}

#[test]
fn nothing_is_kept_when_there_is_no_take() {
    let mut session = a_session(vec![read("one")]);

    assert!(!session.keep("one"));

    assert_eq!(session.prompt(), Some(&read("one")));
    assert!(session.finish().samples.is_empty());
}

#[test]
fn after_the_last_prompt_there_is_none_and_a_stray_recording_is_ignored() {
    let mut session = a_session(vec![read("one")]);
    session.recorded(a_take(0.25));
    session.keep("one");

    session.recorded(a_take(0.5));

    assert_eq!(session.prompt(), None);
    assert_eq!(session.take(), None);
    assert_eq!(session.progress(), (1, 1));
    assert_eq!(session.finish().samples.len(), 1);
}

#[test]
fn going_back_returns_to_the_previous_prompt_and_what_was_kept_there() {
    let mut session = a_session(vec![read("one"), read("two")]);
    session.recorded(a_take(0.25));
    session.keep("one");

    session.back();

    assert_eq!(session.prompt(), Some(&read("one")));
    assert_eq!(session.progress(), (0, 2));
    assert_eq!(
        session.kept_here().map(|sample| sample.said.as_str()),
        Some("one")
    );
    assert_eq!(session.take(), None);
}

#[test]
fn keeping_a_new_take_on_a_revisited_prompt_replaces_its_sample() {
    let mut session = a_session(vec![read("one"), read("two")]);
    session.recorded(a_take(0.25));
    session.keep("one, first try");
    session.back();

    session.recorded(a_take(0.5));
    session.keep("one, second try");

    assert_eq!(session.kept_count(), 1);
    let set = session.finish();
    assert_eq!(set.samples.len(), 1);
    assert_eq!(set.samples[0].said, "one, second try");
    assert_eq!(set.samples[0].audio, a_take(0.5).audio);
}

#[test]
fn going_back_discards_a_take_not_yet_kept() {
    let mut session = a_session(vec![read("one"), read("two")]);
    session.skip();
    session.recorded(a_take(0.25));

    session.back();

    assert_eq!(session.prompt(), Some(&read("one")));
    assert_eq!(session.take(), None);
    assert_eq!(session.kept_count(), 0);
}

#[test]
fn going_back_from_the_first_prompt_stays_on_it() {
    let mut session = a_session(vec![read("one"), read("two")]);

    session.back();

    assert_eq!(session.progress(), (0, 2));
}

#[test]
fn moving_past_a_prompt_that_was_kept_before_keeps_its_sample() {
    let mut session = a_session(vec![read("one"), read("two")]);
    session.recorded(a_take(0.25));
    session.keep("one");
    session.back();

    session.skip();

    assert_eq!(session.prompt(), Some(&read("two")));
    assert_eq!(session.kept_here(), None);
    assert_eq!(session.finish().samples.len(), 1);
}

#[test]
fn samples_come_out_in_prompt_order_however_they_were_recorded() {
    let mut session = a_session(vec![read("one"), read("two"), read("three")]);
    session.skip();
    session.recorded(a_take(0.2));
    session.keep("two");
    session.back();
    session.back();
    session.recorded(a_take(0.1));
    session.keep("one");
    session.skip();
    session.recorded(a_take(0.3));
    session.keep("three");

    let said: Vec<String> = session
        .finish()
        .samples
        .into_iter()
        .map(|sample| sample.said)
        .collect();

    assert_eq!(said, ["one", "two", "three"]);
}

#[test]
fn going_back_from_the_end_reopens_the_last_prompt() {
    let mut session = a_session(vec![read("one")]);
    session.recorded(a_take(0.25));
    session.keep("one");

    session.back();

    assert_eq!(session.prompt(), Some(&read("one")));
}

#[test]
fn next_keeps_a_waiting_take_and_moves_on() {
    let mut session = a_session(vec![read("one"), read("two")]);
    session.recorded(a_take(0.25));

    session.next("one");

    assert_eq!(session.prompt(), Some(&read("two")));
    assert_eq!(session.take(), None);
    let set = session.finish();
    assert_eq!(set.samples.len(), 1);
    assert_eq!(set.samples[0].said, "one");
    assert_eq!(set.samples[0].audio, a_take(0.25).audio);
}

#[test]
fn next_with_no_take_waiting_moves_on_and_keeps_what_was_there() {
    let mut session = a_session(vec![read("one"), read("two")]);
    session.recorded(a_take(0.25));
    session.keep("one");
    session.back();

    session.next("ignored");

    assert_eq!(session.prompt(), Some(&read("two")));
    let set = session.finish();
    assert_eq!(set.samples.len(), 1);
    assert_eq!(set.samples[0].said, "one");
}
