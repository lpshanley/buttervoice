use buttervoice_core::DictationId;
use buttervoice_core::key_dictation::FinishedDictation;
use buttervoice_core::language::Language;
use buttervoice_core::recording::StopReason;
use buttervoice_core::timeline::{Mark, Timeline, Timestamp};
use buttervoice_core::transcription::{Provider, Transcript, Transcription};
use buttervoice_core::typing::{Typing, TypingFailure};
use buttervoice_telemetry::{
    file_subscriber, recent_dictations, record_app_quit, record_dictation,
};

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

/// A dictation of "Hello there." typed 612.5 ms after the key came up.
fn typed(id: u64) -> FinishedDictation {
    let mut timeline = Timeline::new(DictationId(id));
    timeline.mark(Mark::KeyDown, at_micros(1_000_000));
    timeline.mark(Mark::KeyUp, at_micros(4_000_000));
    timeline.mark(Mark::TextTyped, at_micros(4_612_500));
    FinishedDictation {
        summary: timeline.summary(),
        stopped_because: StopReason::KeyReleased,
        audio: None,
        input: None,
        provider: Some(Provider::Local),
        backend: Some("parakeet-ultra"),
        language: Some(Language::English),
        transcription: Transcription::Succeeded(Transcript {
            text: "Hello there.".to_string(),
        }),
        typing: Typing::Typed,
    }
}

fn logged(write: impl FnOnce()) -> tempfile::TempDir {
    let log_dir = tempfile::tempdir().unwrap();
    let (subscriber, guard) = file_subscriber(log_dir.path()).unwrap();
    tracing::subscriber::with_default(subscriber, write);
    drop(guard);
    log_dir
}

#[test]
fn the_dictations_in_the_log_are_read_back_newest_first() {
    let log_dir = logged(|| {
        record_dictation(&typed(1));
        record_app_quit();
        record_dictation(&typed(2));
        record_dictation(&typed(3));
    });

    let recent = recent_dictations(log_dir.path(), 10);

    assert_eq!(recent.iter().map(|d| d.id).collect::<Vec<_>>(), [3, 2, 1]);
    assert_eq!(recent[0].release_to_text_ms, Some(612.5));
    assert_eq!(recent[0].backend.as_deref(), Some("parakeet-ultra"));
    assert_eq!(recent[0].characters, Some(12));
    assert_eq!(recent[0].typing, "typed");
}

#[test]
fn only_the_most_recent_are_read_back() {
    let log_dir = logged(|| (1..=5).for_each(|id| record_dictation(&typed(id))));

    let recent = recent_dictations(log_dir.path(), 2);

    assert_eq!(recent.iter().map(|d| d.id).collect::<Vec<_>>(), [5, 4]);
}

#[test]
fn a_dictation_that_could_not_be_typed_says_so() {
    let log_dir = logged(|| {
        record_dictation(&FinishedDictation {
            typing: Typing::Failed(TypingFailure::NotPermitted),
            ..typed(1)
        })
    });

    let recent = recent_dictations(log_dir.path(), 10);

    assert_eq!(recent[0].typing, "failed");
}

#[test]
fn a_folder_with_no_log_has_no_dictations() {
    let empty = tempfile::tempdir().unwrap();
    assert!(recent_dictations(empty.path(), 10).is_empty());
}
