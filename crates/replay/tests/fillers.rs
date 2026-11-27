use std::time::Duration;

use buttervoice_core::capture::InputKind;
use buttervoice_core::language::Language;
use buttervoice_replay::fillers::{WronglyDropped, check_fillers};
use buttervoice_replay::summary::Scored;
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
fn each_sample_is_scored_again_with_its_thinking_sounds_taken_out() {
    let check = check_fillers(&[heard(
        Kind::Fillers,
        "I think we should ship it on Friday.",
        "Um, I think we should, uh, ship it on Friday.",
    )]);

    assert_eq!(
        check.after[0].heard,
        Ok("I think we should ship it on Friday.".to_string())
    );
    assert_eq!(check.dropped, 2);
    assert_eq!(check.wrongly_dropped, []);
}

#[test]
fn a_word_the_filter_took_that_was_really_said_is_listed() {
    let said = "Take him to the ER, then come back.";
    let check = check_fillers(&[heard(
        Kind::ReadSentence,
        said,
        "take him to the er, then come back",
    )]);

    assert_eq!(
        check.wrongly_dropped,
        [WronglyDropped {
            said: said.to_string(),
            word: "er".to_string(),
        }]
    );
}
