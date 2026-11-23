use std::time::Duration;

use crate::DictationId;
use crate::capture::{AudioStats, Captured, CapturedAudio, InputKind};
use crate::hotkey::{HotkeyDecoder, HotkeyKey, KeyEdge};
use crate::language::Language;
use crate::recording::{Decision, Mode, RecordingControl, StopReason};
use crate::timeline::{Mark, Summary, Timeline, Timestamp};
use crate::transcription::{Provider, Transcript, Transcription, TranscriptionFailure};
use crate::typing::{Typing, TypingFailure};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureCommand {
    Start,
    Stop,
}

/// What the app is doing for the person dictating, for it to show them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationStatus {
    Idle,
    /// The key is down but no sound has come from the microphone yet.
    Starting,
    Listening,
    /// The key is up and the words are on their way.
    Working,
}

/// What a dictation is for. It is fixed when the dictation's key goes
/// down, so nothing that changes while the key is held can type it or send
/// it somewhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Typed into the app in front, heard by this provider.
    Dictation(Provider),
    /// A take for a sample session: never typed, and never heard anywhere
    /// but this Mac.
    SampleTake,
}

impl Purpose {
    /// Whether this provider may hear a dictation made for this purpose.
    pub fn allows(self, provider: Provider) -> bool {
        match self {
            Purpose::Dictation(chosen) => provider == chosen,
            Purpose::SampleTake => provider == Provider::Local,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AfterCapture {
    Transcribe(CapturedAudio, Purpose),
    Finished(FinishedDictation),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AfterTranscription {
    Type(String),
    Finished(FinishedDictation),
}

/// `audio` is `None` when the microphone could not be started.
#[derive(Debug, Clone, PartialEq)]
pub struct FinishedDictation {
    pub summary: Summary,
    pub stopped_because: StopReason,
    pub audio: Option<AudioStats>,
    pub input: Option<InputKind>,
    pub provider: Option<Provider>,
    /// The catalog id of what transcribed it, such as `parakeet-ultra`.
    pub backend: Option<&'static str>,
    pub language: Option<Language>,
    pub transcription: Transcription,
    pub typing: Typing,
}

#[derive(Debug)]
struct Stopped {
    timeline: Timeline,
    purpose: Purpose,
    stopped_because: StopReason,
    input: Option<InputKind>,
    provider: Option<Provider>,
    backend: Option<&'static str>,
    language: Option<Language>,
}

#[derive(Debug)]
enum Phase {
    Idle,
    Capturing {
        timeline: Timeline,
        purpose: Purpose,
        heard_sound: bool,
    },
    StoppingCapture(Stopped),
    Transcribing(Stopped, AudioStats),
    Typing(Stopped, AudioStats, Transcript),
}

/// Held no longer than this, a press is a tap rather than speech.
const LONGEST_TAP: Duration = Duration::from_millis(300);
/// How soon after a tap the second one must come to go hands-free.
const SECOND_TAP_WITHIN: Duration = Duration::from_millis(400);

#[derive(Debug)]
pub struct KeyDictations {
    decoder: HotkeyDecoder,
    control: RecordingControl,
    phase: Phase,
    pressed_at: Option<Timestamp>,
    second_tap_by: Option<Timestamp>,
    purpose_from_now: Purpose,
    last_dictation_text: Option<String>,
}

impl KeyDictations {
    pub fn new(key: HotkeyKey, mode: Mode) -> Self {
        Self {
            decoder: HotkeyDecoder::new(key),
            control: RecordingControl::new(mode),
            phase: Phase::Idle,
            pressed_at: None,
            second_tap_by: None,
            purpose_from_now: Purpose::Dictation(Provider::Local),
            last_dictation_text: None,
        }
    }

    pub fn set_key(&mut self, key: HotkeyKey) {
        self.decoder.set_key(key);
    }

    pub fn status(&self) -> DictationStatus {
        match self.phase {
            Phase::Idle => DictationStatus::Idle,
            Phase::Capturing {
                heard_sound: false, ..
            } => DictationStatus::Starting,
            Phase::Capturing { .. } => DictationStatus::Listening,
            Phase::StoppingCapture(_) | Phase::Transcribing(..) | Phase::Typing(..) => {
                DictationStatus::Working
            }
        }
    }

    /// What a dictation begun from now on is for. One already begun keeps
    /// its own.
    pub fn set_purpose(&mut self, purpose: Purpose) {
        self.purpose_from_now = purpose;
    }

    /// The text of the last dictation, typed or not, held only in memory.
    pub fn last_dictation_text(&self) -> Option<&str> {
        self.last_dictation_text.as_deref()
    }

    pub fn latest_dictation(&self) -> Option<DictationId> {
        self.control.latest_dictation()
    }

    /// The microphone has delivered its first real sound for the dictation
    /// being captured.
    pub fn sound_arrived(&mut self) {
        if let Phase::Capturing { heard_sound, .. } = &mut self.phase {
            *heard_sound = true;
        }
    }

    pub fn flags_changed(&mut self, flags: u64, at: Timestamp) -> Option<CaptureCommand> {
        let edge = self.decoder.flags_changed(flags)?;
        self.key_edge(edge, at)
    }

    /// Some other key went down. The hotkey held with it, as Option is for
    /// an accent, was a shortcut and not a tap.
    pub fn other_key_pressed(&mut self) {
        self.pressed_at = None;
    }

    /// When a tap still waiting for a second one should give up, for the
    /// app to call `second_tap_window_closed` then.
    pub fn second_tap_deadline(&self) -> Option<Timestamp> {
        self.second_tap_by
    }

    pub fn second_tap_window_closed(&mut self, at: Timestamp) -> Option<CaptureCommand> {
        if self.second_tap_by.is_none_or(|deadline| at < deadline) {
            return None;
        }
        self.second_tap_by = None;
        let decision = self.control.second_tap_missed();
        self.apply(decision, at)
    }

    /// `flags` is the modifier state read directly from macOS after key
    /// events may have been missed.
    pub fn resync(&mut self, flags: u64, at: Timestamp) -> Option<CaptureCommand> {
        let edge = self.decoder.resync(flags)?;
        self.key_edge(edge, at)
    }

    pub fn key_listener_lost(&mut self, at: Timestamp) -> Option<CaptureCommand> {
        let decision = self.control.key_listener_lost();
        self.apply(decision, at)
    }

    /// Ends the dictation being captured, as the menu does when the key
    /// listener missed a release.
    pub fn stop_by_hand(&mut self, at: Timestamp) -> Option<CaptureCommand> {
        let decision = self.control.manual_stop();
        self.apply(decision, at)
    }

    /// `captured` is `None` when the microphone could not be started.
    pub fn capture_stopped(
        &mut self,
        captured: Option<Captured>,
        at: Timestamp,
    ) -> Option<AfterCapture> {
        let Phase::StoppingCapture(mut stopped) = std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        stopped.timeline.mark(Mark::CaptureStopped, at);
        let Some(captured) = captured else {
            return Some(AfterCapture::Finished(self.finish(
                stopped,
                None,
                Transcription::NotAttempted,
                Typing::NotAttempted,
            )));
        };
        stopped.input = Some(captured.input);
        if let Some(at) = captured.report.first_sample_at {
            stopped.timeline.mark(Mark::FirstSample, at);
        }
        if let Some(at) = captured.report.first_sound_at {
            stopped.timeline.mark(Mark::FirstSound, at);
        }
        let stats = captured.audio.stats();
        if captured.report.first_sound_at.is_none() {
            return Some(AfterCapture::Finished(self.finish(
                stopped,
                Some(stats),
                Transcription::NotAttempted,
                Typing::NotAttempted,
            )));
        }
        let purpose = stopped.purpose;
        self.phase = Phase::Transcribing(stopped, stats);
        Some(AfterCapture::Transcribe(captured.audio, purpose))
    }

    pub fn transcription_finished(
        &mut self,
        provider: Provider,
        backend: &'static str,
        language: Language,
        outcome: Result<Transcript, TranscriptionFailure>,
        at: Timestamp,
    ) -> Option<AfterTranscription> {
        let Phase::Transcribing(mut stopped, stats) =
            std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        stopped.timeline.mark(Mark::TranscriptReady, at);
        stopped.provider = Some(provider);
        stopped.backend = Some(backend);
        stopped.language = Some(language);
        let transcript = match outcome {
            Ok(transcript) if stopped.purpose == Purpose::SampleTake => {
                return Some(AfterTranscription::Finished(self.finish(
                    stopped,
                    Some(stats),
                    Transcription::Succeeded(transcript),
                    Typing::NotAttempted,
                )));
            }
            Ok(transcript) if !transcript.text.is_empty() => transcript,
            Ok(blank) => {
                return Some(AfterTranscription::Finished(self.finish(
                    stopped,
                    Some(stats),
                    Transcription::Succeeded(blank),
                    Typing::NotAttempted,
                )));
            }
            Err(failure) => {
                return Some(AfterTranscription::Finished(self.finish(
                    stopped,
                    Some(stats),
                    Transcription::Failed(failure),
                    Typing::NotAttempted,
                )));
            }
        };
        let text = transcript.text.clone();
        self.last_dictation_text = Some(text.clone());
        self.phase = Phase::Typing(stopped, stats, transcript);
        Some(AfterTranscription::Type(text))
    }

    /// For a dictation whose text is wanted but must not be typed, such as
    /// a recording made as a sample.
    pub fn transcription_kept(
        &mut self,
        provider: Provider,
        backend: &'static str,
        language: Language,
        outcome: Result<Transcript, TranscriptionFailure>,
        at: Timestamp,
    ) -> Option<FinishedDictation> {
        let Phase::Transcribing(mut stopped, stats) =
            std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        stopped.timeline.mark(Mark::TranscriptReady, at);
        stopped.provider = Some(provider);
        stopped.backend = Some(backend);
        stopped.language = Some(language);
        let transcription = match outcome {
            Ok(transcript) => Transcription::Succeeded(transcript),
            Err(failure) => Transcription::Failed(failure),
        };
        Some(self.finish(stopped, Some(stats), transcription, Typing::NotAttempted))
    }

    /// For a recording that is kept without anything being asked to
    /// transcribe it.
    pub fn kept_untranscribed(&mut self) -> Option<FinishedDictation> {
        let Phase::Transcribing(stopped, stats) = std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        Some(self.finish(
            stopped,
            Some(stats),
            Transcription::NotAttempted,
            Typing::NotAttempted,
        ))
    }

    pub fn typing_finished(
        &mut self,
        outcome: Result<(), TypingFailure>,
        at: Timestamp,
    ) -> Option<FinishedDictation> {
        let Phase::Typing(mut stopped, stats, transcript) =
            std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        let typing = match outcome {
            Ok(()) => {
                stopped.timeline.mark(Mark::TextTyped, at);
                Typing::Typed
            }
            Err(failure) => Typing::Failed(failure),
        };
        Some(self.finish(
            stopped,
            Some(stats),
            Transcription::Succeeded(transcript),
            typing,
        ))
    }

    fn finish(
        &mut self,
        stopped: Stopped,
        audio: Option<AudioStats>,
        transcription: Transcription,
        typing: Typing,
    ) -> FinishedDictation {
        self.control.processing_finished();
        FinishedDictation {
            summary: stopped.timeline.summary(),
            stopped_because: stopped.stopped_because,
            audio,
            input: stopped.input,
            provider: stopped.provider,
            backend: stopped.backend,
            language: stopped.language,
            transcription,
            typing,
        }
    }

    fn key_edge(&mut self, edge: KeyEdge, at: Timestamp) -> Option<CaptureCommand> {
        let decision = match edge {
            KeyEdge::Pressed(press) => {
                // The window may have closed before the app had a chance to say so.
                if let Some(command) = self.second_tap_window_closed(at) {
                    return Some(command);
                }
                self.pressed_at = Some(at);
                self.second_tap_by = None;
                self.control.key_pressed(press)
            }
            KeyEdge::Released(press) => {
                let held = self
                    .pressed_at
                    .take()
                    .map(|down| at.saturating_duration_since(down));
                if held.is_some_and(|held| held <= LONGEST_TAP) {
                    let decision = self.control.key_tapped(press);
                    self.second_tap_by = self.control.awaiting_second_tap().then(|| {
                        Timestamp::from_nanos(at.as_nanos() + SECOND_TAP_WITHIN.as_nanos() as u64)
                    });
                    decision
                } else {
                    self.control.key_released(press)
                }
            }
        };
        self.apply(decision, at)
    }

    fn apply(&mut self, decision: Decision, at: Timestamp) -> Option<CaptureCommand> {
        match decision {
            Decision::Start(dictation) => {
                let mut timeline = Timeline::new(dictation);
                timeline.mark(Mark::KeyDown, at);
                self.phase = Phase::Capturing {
                    timeline,
                    purpose: self.purpose_from_now,
                    heard_sound: false,
                };
                Some(CaptureCommand::Start)
            }
            Decision::Stop(_, stopped_because) => {
                let Phase::Capturing {
                    mut timeline,
                    purpose,
                    ..
                } = std::mem::replace(&mut self.phase, Phase::Idle)
                else {
                    return None;
                };
                timeline.mark(Mark::KeyUp, at);
                self.phase = Phase::StoppingCapture(Stopped {
                    timeline,
                    purpose,
                    stopped_because,
                    input: None,
                    provider: None,
                    backend: None,
                    language: None,
                });
                Some(CaptureCommand::Stop)
            }
            Decision::Ignore => None,
        }
    }
}
