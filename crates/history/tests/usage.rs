use std::time::Duration;

use buttervoice_core::DictationId;
use buttervoice_core::capture::{AudioFormat, AudioStats};
use buttervoice_core::key_dictation::FinishedDictation;
use buttervoice_core::recording::StopReason;
use buttervoice_core::timeline::Timeline;
use buttervoice_core::transcription::{Transcript, Transcription};
use buttervoice_core::typing::Typing;
use buttervoice_history::usage::{Day, Usage};

fn said(text: &str, spoken: Duration, typing: Typing) -> FinishedDictation {
    FinishedDictation {
        summary: Timeline::new(DictationId(1)).summary(),
        stopped_because: StopReason::KeyReleased,
        audio: Some(AudioStats {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 1,
            },
            duration: spoken,
            dropped_samples: 0,
            peak: 0.5,
            rms: 0.1,
        }),
        input: None,
        provider: None,
        backend: None,
        language: None,
        transcription: Transcription::Succeeded(Transcript {
            text: text.to_string(),
        }),
        dropped_fillers: None,
        typing,
    }
}

fn reopened(file: &std::path::Path) -> std::collections::BTreeMap<String, Day> {
    Usage::open(file.to_path_buf()).days()
}

#[test]
fn a_dictation_counts_its_words_and_time_spoken_on_its_day() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("usage.json");
    let usage = Usage::open(file.clone());

    usage
        .count(
            "2026-11-22",
            &said(
                "Vamos lançar a versão amanhã.",
                Duration::from_millis(2_500),
                Typing::Typed,
            ),
        )
        .unwrap();

    let days = reopened(&file);
    assert_eq!(
        days.get("2026-11-22"),
        Some(&Day {
            dictations: 1,
            words: 5,
            spoken_ms: 2_500
        })
    );
}

#[test]
fn dictations_on_one_day_add_up_and_other_days_stay_apart() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("usage.json");
    let usage = Usage::open(file.clone());

    usage
        .count(
            "2026-11-21",
            &said("Good morning", Duration::from_secs(1), Typing::Typed),
        )
        .unwrap();
    usage
        .count(
            "2026-11-22",
            &said("Ship it", Duration::from_secs(1), Typing::Typed),
        )
        .unwrap();
    usage
        .count(
            "2026-11-22",
            &said("Once CI is green", Duration::from_secs(2), Typing::Typed),
        )
        .unwrap();

    let days = reopened(&file);
    assert_eq!(days.len(), 2);
    assert_eq!(
        days["2026-11-21"],
        Day {
            dictations: 1,
            words: 2,
            spoken_ms: 1_000
        }
    );
    assert_eq!(
        days["2026-11-22"],
        Day {
            dictations: 2,
            words: 6,
            spoken_ms: 3_000
        }
    );
}

#[test]
fn a_dictation_never_meant_to_be_typed_is_not_counted() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("usage.json");
    let usage = Usage::open(file.clone());

    let sample_take = said(
        "Read this sentence aloud",
        Duration::from_secs(3),
        Typing::NotAttempted,
    );
    usage.count("2026-11-22", &sample_take).unwrap();

    assert!(reopened(&file).is_empty());
}

#[test]
fn time_saved_is_the_typing_time_less_the_time_spent_speaking() {
    // 400 words at 40 a minute take 10 minutes to type; saying them took 3.
    let saved = buttervoice_history::usage::time_saved(400, 3 * 60_000, 40);
    assert_eq!(saved, Duration::from_secs(7 * 60));
}

#[test]
fn speaking_slower_than_typing_saves_no_time_rather_than_less_than_none() {
    // 10 words at 40 a minute take 15 seconds to type; saying them took 60.
    assert_eq!(
        buttervoice_history::usage::time_saved(10, 60_000, 40),
        Duration::ZERO
    );
}
