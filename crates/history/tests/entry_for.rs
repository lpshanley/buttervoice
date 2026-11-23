use buttervoice_core::DictationId;
use buttervoice_core::key_dictation::FinishedDictation;
use buttervoice_core::language::Language;
use buttervoice_core::recording::StopReason;
use buttervoice_core::timeline::Timeline;
use buttervoice_core::transcription::{Provider, Transcript, Transcription, TranscriptionFailure};
use buttervoice_core::typing::{Typing, TypingFailure};
use buttervoice_history::{App, Entry, entry_for};

fn finished(transcription: Transcription, typing: Typing) -> FinishedDictation {
    FinishedDictation {
        summary: Timeline::new(DictationId(1)).summary(),
        stopped_because: StopReason::KeyReleased,
        audio: None,
        input: None,
        provider: Some(Provider::Local),
        backend: Some("parakeet-ultra"),
        language: Some(Language::BrazilianPortuguese),
        transcription,
        typing,
    }
}

fn heard(text: &str) -> Transcription {
    Transcription::Succeeded(Transcript {
        text: text.to_string(),
    })
}

fn notes_app() -> App {
    App {
        name: "Notes".to_string(),
        bundle_id: Some("com.apple.Notes".to_string()),
    }
}

#[test]
fn a_typed_dictation_is_kept_with_where_it_went() {
    let entry = entry_for(
        &finished(heard("Bom dia"), Typing::Typed),
        1_700_000_000,
        Some(notes_app()),
    );
    assert_eq!(
        entry,
        Some(Entry {
            text: "Bom dia".to_string(),
            at: 1_700_000_000,
            language: Some("pt-BR".to_string()),
            backend: Some("parakeet-ultra".to_string()),
            app: Some(notes_app()),
        })
    );
}

#[test]
fn a_dictation_that_could_not_be_typed_is_kept_so_it_can_be_found() {
    let not_typed = finished(
        heard("Bom dia"),
        Typing::Failed(TypingFailure::NotPermitted),
    );
    assert!(entry_for(&not_typed, 1, None).is_some());
}

#[test]
fn a_dictation_that_was_never_to_be_typed_is_not_kept() {
    // Sample takes and blank results are transcribed but never typed.
    let sample = finished(heard("Can you send me the notes"), Typing::NotAttempted);
    assert_eq!(entry_for(&sample, 1, None), None);
}

#[test]
fn a_dictation_that_was_not_heard_is_not_kept() {
    let failed = finished(
        Transcription::Failed(TranscriptionFailure::Unreachable),
        Typing::NotAttempted,
    );
    assert_eq!(entry_for(&failed, 1, None), None);
}
