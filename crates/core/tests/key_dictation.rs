use std::time::Duration;

use buttervoice_core::DictationId;
use buttervoice_core::capture::{
    AudioFormat, AudioStats, CaptureReport, Captured, CapturedAudio, InputKind,
};
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{
    AfterCapture, AfterTranscription, CaptureCommand, DictationStatus, FinishedDictation,
    KeyDictations,
};
use buttervoice_core::language::Language;
use buttervoice_core::recording::{Mode, StopReason};
use buttervoice_core::timeline::Timestamp;
use buttervoice_core::transcription::{Provider, Transcript, Transcription, TranscriptionFailure};
use buttervoice_core::typing::{Typing, TypingFailure};

const RIGHT_OPTION_DOWN: u64 = 0x80140;
const NO_MODIFIERS: u64 = 0x100;

fn at_micros(micros: u64) -> Timestamp {
    Timestamp::from_nanos(micros * 1_000)
}

fn half_a_second_of_audio() -> CapturedAudio {
    CapturedAudio {
        format: AudioFormat {
            sample_rate: 48_000,
            channels: 1,
        },
        samples: vec![0.25; 24_000],
        dropped_samples: 3,
    }
}

fn speech() -> Captured {
    Captured {
        audio: half_a_second_of_audio(),
        report: CaptureReport {
            first_sample_at: Some(at_micros(1_084_000)),
            first_sound_at: Some(at_micros(1_095_000)),
        },
        input: InputKind::BuiltIn,
    }
}

fn finished(after: Option<AfterCapture>) -> FinishedDictation {
    match after {
        Some(AfterCapture::Finished(dictation)) => dictation,
        other => panic!("expected a finished dictation, got {other:?}"),
    }
}

fn finished_without_typing(after: Option<AfterTranscription>) -> FinishedDictation {
    match after {
        Some(AfterTranscription::Finished(dictation)) => dictation,
        other => panic!("expected a finished dictation, got {other:?}"),
    }
}

fn heard(text: &str) -> Result<Transcript, TranscriptionFailure> {
    Ok(Transcript {
        text: text.to_string(),
    })
}

/// Key down at 1.000000 s and up at 1.956365 s, in push-to-talk mode.
fn held_and_released() -> KeyDictations {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));
    dictations.flags_changed(NO_MODIFIERS, at_micros(1_956_365));
    dictations
}

#[test]
fn pressing_the_key_starts_capture_and_releasing_it_stops_capture() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);

    assert_eq!(
        dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000)),
        Some(CaptureCommand::Start)
    );
    assert_eq!(
        dictations.flags_changed(NO_MODIFIERS, at_micros(1_956_365)),
        Some(CaptureCommand::Stop)
    );
}

#[test]
fn captured_speech_is_handed_over_for_transcription() {
    let mut dictations = held_and_released();

    assert_eq!(
        dictations.capture_stopped(Some(speech()), at_micros(1_980_000)),
        Some(AfterCapture::Transcribe(half_a_second_of_audio()))
    );
}

#[test]
fn a_transcript_is_handed_over_to_be_typed() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    assert_eq!(
        dictations.transcription_finished(
            Provider::Grok,
            "grok",
            Language::English,
            heard("hello world"),
            at_micros(2_392_500)
        ),
        Some(AfterTranscription::Type("hello world".to_string()))
    );
}

#[test]
fn a_dictation_finishes_once_its_text_has_been_typed() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));
    dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        heard("hello world"),
        at_micros(2_392_500),
    );

    let finished = dictations
        .typing_finished(Ok(()), at_micros(2_400_000))
        .expect("typing the text should finish the dictation");

    assert_eq!(finished.typing, Typing::Typed);
    assert_eq!(
        finished.transcription,
        Transcription::Succeeded(Transcript {
            text: "hello world".to_string()
        })
    );
    assert_eq!(
        finished.summary.transcription,
        Some(Duration::from_micros(412_500))
    );
    assert_eq!(
        finished.summary.key_up_to_text_typed,
        Some(Duration::from_micros(443_635))
    );
}

#[test]
fn a_dictation_whose_text_could_not_be_typed_still_finishes_and_says_why() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));
    dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        heard("hello world"),
        at_micros(2_392_500),
    );

    let finished = dictations
        .typing_finished(Err(TypingFailure::NotPermitted), at_micros(2_400_000))
        .expect("the failure should finish the dictation");

    assert_eq!(finished.typing, Typing::Failed(TypingFailure::NotPermitted));
    assert_eq!(finished.summary.key_up_to_text_typed, None);
}

#[test]
fn a_blank_transcript_finishes_the_dictation_with_nothing_to_type() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = finished_without_typing(dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        heard(""),
        at_micros(2_392_500),
    ));

    assert_eq!(finished.typing, Typing::NotAttempted);
}

#[test]
fn a_new_press_is_ignored_until_the_text_has_been_typed() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));
    dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        heard("hello world"),
        at_micros(2_392_500),
    );

    assert_eq!(
        dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(2_395_000)),
        None
    );
}

#[test]
fn a_finished_dictation_describes_its_capture() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = finished_without_typing(dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        Err(TranscriptionFailure::Unavailable),
        at_micros(2_000_000),
    ));

    assert_eq!(finished.summary.dictation_id, DictationId(1));
    assert_eq!(finished.stopped_because, StopReason::KeyReleased);
    assert_eq!(
        finished.summary.key_held,
        Some(Duration::from_micros(956_365))
    );
    assert_eq!(
        finished.summary.key_down_to_first_sample,
        Some(Duration::from_micros(84_000))
    );
    assert_eq!(
        finished.summary.key_down_to_first_sound,
        Some(Duration::from_micros(95_000))
    );
    assert_eq!(
        finished.summary.key_up_to_capture_stopped,
        Some(Duration::from_micros(23_635))
    );
    assert_eq!(
        finished.audio,
        Some(AudioStats {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 1,
            },
            duration: Duration::from_millis(500),
            dropped_samples: 3,
            peak: 0.25,
            rms: 0.25,
        })
    );
}

#[test]
fn a_failed_transcription_still_finishes_the_dictation_and_says_why() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = finished_without_typing(dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        Err(TranscriptionFailure::Unauthorized),
        at_micros(2_100_000),
    ));

    assert_eq!(
        finished.transcription,
        Transcription::Failed(TranscriptionFailure::Unauthorized)
    );
}

#[test]
fn audio_with_no_real_sound_is_not_sent_for_transcription() {
    let mut dictations = held_and_released();
    let silence = Captured {
        report: CaptureReport {
            first_sample_at: Some(at_micros(1_084_000)),
            first_sound_at: None,
        },
        ..speech()
    };

    let finished = finished(dictations.capture_stopped(Some(silence), at_micros(1_980_000)));

    assert_eq!(finished.transcription, Transcription::NotAttempted);
}

#[test]
fn a_microphone_that_never_started_finishes_the_dictation_without_audio() {
    let mut dictations = held_and_released();

    let finished = finished(dictations.capture_stopped(None, at_micros(1_980_000)));

    assert_eq!(finished.audio, None);
    assert_eq!(finished.transcription, Transcription::NotAttempted);
}

#[test]
fn a_new_press_is_ignored_until_transcription_has_finished() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    assert_eq!(
        dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(2_000_000)),
        None
    );
}

#[test]
fn a_release_missed_by_the_listener_stops_capture_on_resync() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));

    assert_eq!(
        dictations.resync(NO_MODIFIERS, at_micros(1_500_000)),
        Some(CaptureCommand::Stop)
    );
    let finished = finished(dictations.capture_stopped(None, at_micros(1_520_000)));
    assert_eq!(
        finished.summary.key_held,
        Some(Duration::from_micros(500_000))
    );
    assert_eq!(finished.stopped_because, StopReason::KeyReleased);
}

#[test]
fn losing_the_key_listener_stops_capture_and_the_dictation_says_why() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));

    assert_eq!(
        dictations.key_listener_lost(at_micros(1_250_000)),
        Some(CaptureCommand::Stop)
    );
    let finished = finished(dictations.capture_stopped(None, at_micros(1_270_000)));
    assert_eq!(finished.stopped_because, StopReason::KeyListenerLost);
}

/// One whole dictation with its key going down at `key_down_micros`, up to
/// the text being handed over for typing.
fn dictate(
    dictations: &mut KeyDictations,
    key_down_micros: u64,
    text: &str,
) -> Option<AfterTranscription> {
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(key_down_micros));
    dictations.flags_changed(NO_MODIFIERS, at_micros(key_down_micros + 1_000_000));
    dictations.capture_stopped(Some(speech()), at_micros(key_down_micros + 1_020_000));
    dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        heard(text),
        at_micros(key_down_micros + 1_300_000),
    )
}

/// "Hello there." has been dictated and typed by 2.400000 s.
fn hello_there_typed() -> KeyDictations {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictate(&mut dictations, 1_000_000, "Hello there.");
    dictations.typing_finished(Ok(()), at_micros(2_400_000));
    dictations
}

fn to_type(text: &str) -> Option<AfterTranscription> {
    Some(AfterTranscription::Type(text.to_string()))
}

// Dictations come in quick succession, and a space put in front of one
// turned out to be wrong more often than it was right.
#[test]
fn text_dictated_straight_after_the_last_is_typed_exactly_as_heard() {
    let mut dictations = hello_there_typed();

    assert_eq!(
        dictate(&mut dictations, 2_500_000, "How are you?"),
        to_type("How are you?")
    );
}

#[test]
fn the_status_goes_from_starting_to_listening_to_working_to_idle_over_a_dictation() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    assert_eq!(dictations.status(), DictationStatus::Idle);

    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));
    assert_eq!(dictations.status(), DictationStatus::Starting);

    dictations.sound_arrived();
    assert_eq!(dictations.status(), DictationStatus::Listening);

    dictations.flags_changed(NO_MODIFIERS, at_micros(1_956_365));
    assert_eq!(dictations.status(), DictationStatus::Working);

    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));
    assert_eq!(dictations.status(), DictationStatus::Working);

    dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        heard("hello world"),
        at_micros(2_392_500),
    );
    assert_eq!(dictations.status(), DictationStatus::Working);

    dictations.typing_finished(Ok(()), at_micros(2_400_000));
    assert_eq!(dictations.status(), DictationStatus::Idle);
}

#[test]
fn the_latest_dictation_is_the_one_most_recently_started_finished_or_not() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);

    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));
    assert_eq!(dictations.latest_dictation(), Some(DictationId(1)));

    dictations.flags_changed(NO_MODIFIERS, at_micros(1_956_365));
    finished(dictations.capture_stopped(None, at_micros(1_960_000)));
    assert_eq!(dictations.latest_dictation(), Some(DictationId(1)));

    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(5_000_000));
    assert_eq!(dictations.latest_dictation(), Some(DictationId(2)));
}

#[test]
fn before_the_first_dictation_there_is_no_latest_one() {
    let dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);

    assert_eq!(dictations.latest_dictation(), None);
}

#[test]
fn a_finished_dictation_says_which_kind_of_input_it_was_captured_from() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = finished_without_typing(dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::English,
        Err(TranscriptionFailure::Unreachable),
        at_micros(2_100_000),
    ));

    assert_eq!(finished.input, Some(InputKind::BuiltIn));
}

#[test]
fn a_finished_dictation_says_which_provider_transcribed_it() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = finished_without_typing(dictations.transcription_finished(
        Provider::Local,
        "parakeet-ultra",
        Language::English,
        Err(TranscriptionFailure::EngineFailed),
        at_micros(2_100_000),
    ));

    assert_eq!(finished.provider, Some(Provider::Local));
}

#[test]
fn a_dictation_that_was_never_sent_for_transcription_has_no_provider() {
    let mut dictations = held_and_released();

    let finished = finished(dictations.capture_stopped(None, at_micros(1_980_000)));

    assert_eq!(finished.provider, None);
}

#[test]
fn a_finished_dictation_says_which_language_it_was_transcribed_as() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = finished_without_typing(dictations.transcription_finished(
        Provider::Grok,
        "grok",
        Language::BrazilianPortuguese,
        Err(TranscriptionFailure::Unreachable),
        at_micros(2_100_000),
    ));

    assert_eq!(finished.language, Some(Language::BrazilianPortuguese));
}

#[test]
fn a_dictation_kept_as_a_sample_is_transcribed_but_never_typed() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = dictations
        .transcription_kept(
            Provider::Local,
            "parakeet-ultra",
            Language::English,
            heard("hello world"),
            at_micros(2_392_500),
        )
        .expect("the dictation finishes");

    assert_eq!(
        finished.transcription,
        Transcription::Succeeded(Transcript {
            text: "hello world".to_string()
        })
    );
    assert_eq!(finished.typing, Typing::NotAttempted);
    assert_eq!(finished.provider, Some(Provider::Local));
    assert_eq!(finished.language, Some(Language::English));
    assert_eq!(dictations.status(), DictationStatus::Idle);
}

#[test]
fn a_recording_can_be_kept_without_being_transcribed_at_all() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = dictations
        .kept_untranscribed()
        .expect("the dictation finishes");

    assert_eq!(finished.transcription, Transcription::NotAttempted);
    assert_eq!(finished.typing, Typing::NotAttempted);
    assert_eq!(finished.provider, None);
    assert_eq!(dictations.status(), DictationStatus::Idle);
}

#[test]
fn a_finished_dictation_says_which_model_transcribed_it() {
    let mut dictations = held_and_released();
    dictations.capture_stopped(Some(speech()), at_micros(1_980_000));

    let finished = finished_without_typing(dictations.transcription_finished(
        Provider::Local,
        "parakeet-ultra",
        Language::English,
        Err(TranscriptionFailure::EngineFailed),
        at_micros(2_100_000),
    ));

    assert_eq!(finished.backend, Some("parakeet-ultra"));
}

#[test]
fn a_key_released_before_any_sound_arrived_goes_straight_to_working() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));

    dictations.flags_changed(NO_MODIFIERS, at_micros(1_300_000));

    assert_eq!(dictations.status(), DictationStatus::Working);
}

#[test]
fn sound_arriving_when_nothing_is_being_captured_changes_nothing() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);

    dictations.sound_arrived();
    assert_eq!(dictations.status(), DictationStatus::Idle);

    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));
    dictations.flags_changed(NO_MODIFIERS, at_micros(1_300_000));
    dictations.sound_arrived();
    assert_eq!(dictations.status(), DictationStatus::Working);
}

#[test]
fn each_dictation_waits_for_its_own_first_sound() {
    let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(1_000_000));
    dictations.sound_arrived();
    dictations.flags_changed(NO_MODIFIERS, at_micros(1_956_365));
    finished(dictations.capture_stopped(None, at_micros(1_980_000)));

    dictations.flags_changed(RIGHT_OPTION_DOWN, at_micros(3_000_000));

    assert_eq!(dictations.status(), DictationStatus::Starting);
}
