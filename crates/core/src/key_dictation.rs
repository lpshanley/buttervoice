use crate::DictationId;
use crate::capture::{AudioStats, Captured, CapturedAudio};
use crate::hotkey::{HotkeyDecoder, HotkeyKey, KeyEdge};
use crate::recording::{Decision, Mode, RecordingControl, StopReason};
use crate::timeline::{Mark, Summary, Timeline, Timestamp};
use crate::transcription::{Transcript, Transcription, TranscriptionFailure};
use crate::typing::{Typing, TypingFailure};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureCommand {
    Start,
    Stop,
}

/// What the app is doing for the person dictating, for it to show them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationStatus {
    Idle,
    Listening,
    /// The key is up and the words are on their way.
    Working,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AfterCapture {
    Transcribe(CapturedAudio),
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
    pub transcription: Transcription,
    pub typing: Typing,
}

#[derive(Debug)]
struct Stopped {
    timeline: Timeline,
    stopped_because: StopReason,
    key_down_at: Timestamp,
}

#[derive(Debug)]
struct LastTyped {
    ended_with: char,
    at: Timestamp,
}

/// How soon the key has to go down after the last text was typed for the new
/// text to count as carrying on from it.
const RESUME_WINDOW: Duration = Duration::from_secs(10);

#[derive(Debug)]
enum Phase {
    Idle,
    Capturing {
        timeline: Timeline,
        key_down_at: Timestamp,
    },
    StoppingCapture(Stopped),
    Transcribing(Stopped, AudioStats),
    Typing(Stopped, AudioStats, Transcript),
}

#[derive(Debug)]
pub struct KeyDictations {
    decoder: HotkeyDecoder,
    control: RecordingControl,
    phase: Phase,
    last_typed: Option<LastTyped>,
}

impl KeyDictations {
    pub fn new(key: HotkeyKey, mode: Mode) -> Self {
        Self {
            decoder: HotkeyDecoder::new(key),
            control: RecordingControl::new(mode),
            phase: Phase::Idle,
            last_typed: None,
        }
    }

    pub fn status(&self) -> DictationStatus {
        match self.phase {
            Phase::Idle => DictationStatus::Idle,
            Phase::Capturing { .. } => DictationStatus::Listening,
            Phase::StoppingCapture(_) | Phase::Transcribing(..) | Phase::Typing(..) => {
                DictationStatus::Working
            }
        }
    }

    pub fn latest_dictation(&self) -> Option<DictationId> {
        self.control.latest_dictation()
    }

    pub fn flags_changed(&mut self, flags: u64, at: Timestamp) -> Option<CaptureCommand> {
        let edge = self.decoder.flags_changed(flags)?;
        self.key_edge(edge, at)
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
        self.phase = Phase::Transcribing(stopped, stats);
        Some(AfterCapture::Transcribe(captured.audio))
    }

    pub fn transcription_finished(
        &mut self,
        outcome: Result<Transcript, TranscriptionFailure>,
        at: Timestamp,
    ) -> Option<AfterTranscription> {
        let Phase::Transcribing(mut stopped, stats) =
            std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        stopped.timeline.mark(Mark::TranscriptReady, at);
        let transcript = match outcome {
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
        let text = if self.carries_on_from_the_last_text(stopped.key_down_at, &transcript.text) {
            format!(" {}", transcript.text)
        } else {
            transcript.text.clone()
        };
        self.phase = Phase::Typing(stopped, stats, transcript);
        Some(AfterTranscription::Type(text))
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
                self.last_typed = transcript
                    .text
                    .chars()
                    .last()
                    .map(|ended_with| LastTyped { ended_with, at });
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

    fn carries_on_from_the_last_text(&self, key_down_at: Timestamp, text: &str) -> bool {
        let Some(last) = &self.last_typed else {
            return false;
        };
        let starts_a_word = text
            .chars()
            .next()
            .is_some_and(|first| first.is_alphanumeric() || "\"'(\u{201C}\u{2018}".contains(first));
        key_down_at.saturating_duration_since(last.at) <= RESUME_WINDOW
            && !last.ended_with.is_whitespace()
            && starts_a_word
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
            transcription,
            typing,
        }
    }

    fn key_edge(&mut self, edge: KeyEdge, at: Timestamp) -> Option<CaptureCommand> {
        let decision = match edge {
            KeyEdge::Pressed(press) => self.control.key_pressed(press),
            KeyEdge::Released(press) => self.control.key_released(press),
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
                    key_down_at: at,
                };
                Some(CaptureCommand::Start)
            }
            Decision::Stop(_, stopped_because) => {
                let Phase::Capturing {
                    mut timeline,
                    key_down_at,
                } = std::mem::replace(&mut self.phase, Phase::Idle)
                else {
                    return None;
                };
                timeline.mark(Mark::KeyUp, at);
                self.phase = Phase::StoppingCapture(Stopped {
                    timeline,
                    stopped_because,
                    key_down_at,
                });
                Some(CaptureCommand::Stop)
            }
            Decision::Ignore => None,
        }
    }
}
