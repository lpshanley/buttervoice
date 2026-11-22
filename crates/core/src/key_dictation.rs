use crate::capture::{AudioStats, Captured, CapturedAudio};
use crate::hotkey::{HotkeyDecoder, HotkeyKey, KeyEdge};
use crate::recording::{Decision, Mode, RecordingControl, StopReason};
use crate::timeline::{Mark, Summary, Timeline, Timestamp};
use crate::transcription::{Transcript, Transcription, TranscriptionFailure};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureCommand {
    Start,
    Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AfterCapture {
    Transcribe(CapturedAudio),
    Finished(FinishedDictation),
}

/// `audio` is `None` when the microphone could not be started.
#[derive(Debug, Clone, PartialEq)]
pub struct FinishedDictation {
    pub summary: Summary,
    pub stopped_because: StopReason,
    pub audio: Option<AudioStats>,
    pub transcription: Transcription,
}

#[derive(Debug)]
struct Stopped {
    timeline: Timeline,
    stopped_because: StopReason,
}

#[derive(Debug)]
enum Phase {
    Idle,
    Capturing(Timeline),
    StoppingCapture(Stopped),
    Transcribing(Stopped, AudioStats),
}

#[derive(Debug)]
pub struct KeyDictations {
    decoder: HotkeyDecoder,
    control: RecordingControl,
    phase: Phase,
}

impl KeyDictations {
    pub fn new(key: HotkeyKey, mode: Mode) -> Self {
        Self {
            decoder: HotkeyDecoder::new(key),
            control: RecordingControl::new(mode),
            phase: Phase::Idle,
        }
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
            )));
        }
        self.phase = Phase::Transcribing(stopped, stats);
        Some(AfterCapture::Transcribe(captured.audio))
    }

    pub fn transcription_finished(
        &mut self,
        outcome: Result<Transcript, TranscriptionFailure>,
        at: Timestamp,
    ) -> Option<FinishedDictation> {
        let Phase::Transcribing(mut stopped, stats) =
            std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        stopped.timeline.mark(Mark::TranscriptReady, at);
        let transcription = match outcome {
            Ok(transcript) => Transcription::Succeeded(transcript),
            Err(failure) => Transcription::Failed(failure),
        };
        Some(self.finish(stopped, Some(stats), transcription))
    }

    fn finish(
        &mut self,
        stopped: Stopped,
        audio: Option<AudioStats>,
        transcription: Transcription,
    ) -> FinishedDictation {
        self.control.processing_finished();
        FinishedDictation {
            summary: stopped.timeline.summary(),
            stopped_because: stopped.stopped_because,
            audio,
            transcription,
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
                self.phase = Phase::Capturing(timeline);
                Some(CaptureCommand::Start)
            }
            Decision::Stop(_, stopped_because) => {
                let Phase::Capturing(mut timeline) =
                    std::mem::replace(&mut self.phase, Phase::Idle)
                else {
                    return None;
                };
                timeline.mark(Mark::KeyUp, at);
                self.phase = Phase::StoppingCapture(Stopped {
                    timeline,
                    stopped_because,
                });
                Some(CaptureCommand::Stop)
            }
            Decision::Ignore => None,
        }
    }
}
