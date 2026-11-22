use std::time::Duration;

use buttervoice_core::capture::InputKind;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::TranscriptionFailure;
use buttervoice_replay::summary::{Scored, by, by_kind};
use buttervoice_samples::Kind;

fn heard(kind: Kind, said: &str, heard: &str) -> Scored {
    Scored {
        kind,
        language: Language::English,
        speaker: "speaker-a".to_string(),
        input: InputKind::BuiltIn,
        said: said.to_string(),
        heard: Ok(heard.to_string()),
        audio: Duration::from_secs(2),
        took: Duration::from_millis(20),
    }
}

#[test]
fn a_kind_is_scored_over_all_its_words_together_not_sample_by_sample() {
    let scored = [
        heard(Kind::ShortUtterance, "yes", "yet"),
        heard(
            Kind::ShortUtterance,
            "that all sounds good to me thanks",
            "that all sounds good to me thanks",
        ),
        heard(Kind::ShortUtterance, "not yet", "not yet"),
    ];

    let summary = &by_kind(&scored)[0];

    assert_eq!(summary.0, Kind::ShortUtterance);
    assert_eq!(summary.1.samples, 3);
    assert_eq!(summary.1.word_error_rate, Some(0.1));
    assert_eq!(summary.1.invented_text, 0);
    assert_eq!(summary.1.failures, 0);
}

#[test]
fn silence_has_no_error_rate_and_counts_what_was_written_anyway() {
    let scored = [
        heard(Kind::Silence, "", ""),
        heard(Kind::Silence, "", "Thank you."),
        heard(Kind::Silence, "", "."),
    ];

    let summary = &by_kind(&scored)[0].1;

    assert_eq!(summary.word_error_rate, None);
    // A lone full stop has no words in it, but the app would type it.
    assert_eq!(summary.invented_text, 2);
}

#[test]
fn a_failed_transcription_misses_every_word_and_is_counted_as_a_failure() {
    let scored = [
        heard(Kind::ReadSentence, "one two three", "one two three"),
        Scored {
            heard: Err(TranscriptionFailure::EngineFailed),
            ..heard(Kind::ReadSentence, "four five six", "")
        },
    ];

    let summary = &by_kind(&scored)[0].1;

    assert_eq!(summary.word_error_rate, Some(0.5));
    assert_eq!(summary.failures, 1);
}

#[test]
fn kinds_come_in_their_usual_order_and_only_those_that_were_recorded() {
    let scored = [
        heard(Kind::Silence, "", ""),
        heard(Kind::HardWords, "cache", "cache"),
        heard(Kind::ReadSentence, "hello", "hello"),
    ];

    let kinds: Vec<Kind> = by_kind(&scored).into_iter().map(|(kind, _)| kind).collect();

    assert_eq!(
        kinds,
        vec![Kind::ReadSentence, Kind::HardWords, Kind::Silence]
    );
}

#[test]
fn samples_can_be_read_by_any_label_in_the_order_the_labels_first_appear() {
    let on = |input, said: &str, heard_as: &str| Scored {
        input,
        ..heard(Kind::ReadSentence, said, heard_as)
    };
    let scored = [
        on(InputKind::Other, "one two", "one too"),
        on(InputKind::BuiltIn, "three four", "three four"),
        on(InputKind::Other, "five six", "five six"),
    ];

    let by_input: Vec<(InputKind, Option<f64>)> = by(&scored, |sample| sample.input)
        .into_iter()
        .map(|(input, summary)| (input, summary.word_error_rate))
        .collect();

    assert_eq!(
        by_input,
        vec![
            (InputKind::Other, Some(0.25)),
            (InputKind::BuiltIn, Some(0.0))
        ]
    );
}

#[test]
fn cost_is_seconds_of_audio_per_second_of_work_and_the_median_time_taken() {
    let taking = |seconds, millis| Scored {
        audio: Duration::from_secs(seconds),
        took: Duration::from_millis(millis),
        ..heard(Kind::LongDictation, "hello", "hello")
    };
    let scored = [taking(6, 100), taking(12, 900), taking(6, 200)];

    let summary = &by_kind(&scored)[0].1;

    // 24 seconds of audio in 1.2 seconds of work.
    assert_eq!(summary.times_real_time, Some(20.0));
    assert_eq!(summary.median_took, Duration::from_millis(200));
}
