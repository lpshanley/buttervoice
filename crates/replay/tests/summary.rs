use buttervoice_core::language::Language;
use buttervoice_core::transcription::TranscriptionFailure;
use buttervoice_replay::summary::{KindSummary, Scored, summarise};
use buttervoice_samples::Kind;

fn heard(kind: Kind, said: &str, heard: &str) -> Scored {
    Scored {
        kind,
        language: Language::English,
        said: said.to_string(),
        heard: Ok(heard.to_string()),
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

    assert_eq!(
        summarise(&scored),
        vec![KindSummary {
            kind: Kind::ShortUtterance,
            samples: 3,
            word_error_rate: Some(0.1),
            invented_text: 0,
            failures: 0,
        }]
    );
}

#[test]
fn silence_has_no_error_rate_and_counts_what_was_written_anyway() {
    let scored = [
        heard(Kind::Silence, "", ""),
        heard(Kind::Silence, "", "Thank you."),
        heard(Kind::Silence, "", "."),
    ];

    assert_eq!(
        summarise(&scored),
        vec![KindSummary {
            kind: Kind::Silence,
            samples: 3,
            word_error_rate: None,
            // A lone full stop has no words in it, but the app would type it.
            invented_text: 2,
            failures: 0,
        }]
    );
}

#[test]
fn a_failed_transcription_misses_every_word_and_is_counted_as_a_failure() {
    let scored = [
        heard(Kind::ReadSentence, "one two three", "one two three"),
        Scored {
            kind: Kind::ReadSentence,
            language: Language::English,
            said: "four five six".to_string(),
            heard: Err(TranscriptionFailure::EngineFailed),
        },
    ];

    let summary = summarise(&scored);

    assert_eq!(summary[0].word_error_rate, Some(0.5));
    assert_eq!(summary[0].failures, 1);
}

#[test]
fn kinds_come_in_their_usual_order_and_only_those_that_were_recorded() {
    let scored = [
        heard(Kind::Silence, "", ""),
        heard(Kind::HardWords, "cache", "cache"),
        heard(Kind::ReadSentence, "hello", "hello"),
    ];

    let kinds: Vec<Kind> = summarise(&scored)
        .into_iter()
        .map(|kind| kind.kind)
        .collect();

    assert_eq!(
        kinds,
        vec![Kind::ReadSentence, Kind::HardWords, Kind::Silence]
    );
}
