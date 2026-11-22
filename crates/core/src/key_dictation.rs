use crate::capture::{CaptureReport, Captured, CapturedAudio};
use crate::hotkey::{HotkeyDecoder, HotkeyKey, KeyEdge};
use crate::recording::{Decision, Mode, RecordingControl, StopReason};
use crate::timeline::{Mark, Summary, Timeline, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureCommand {
    Start,
    Stop,
}

/// `audio` is `None` when the microphone could not be started.
#[derive(Debug, Clone, PartialEq)]
pub struct FinishedDictation {
    pub summary: Summary,
    pub stopped_because: StopReason,
    pub audio: Option<CapturedAudio>,
}

#[derive(Debug)]
enum Phase {
    Idle,
    Capturing(Timeline),
    StoppingCapture(Timeline, StopReason),
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

    pub fn capture_stopped(&mut self, captured: Option<Captured>) -> Option<FinishedDictation> {
        let Phase::StoppingCapture(mut timeline, stopped_because) =
            std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return None;
        };
        let (audio, report) = match captured {
            Some(captured) => (Some(captured.audio), captured.report),
            None => (None, CaptureReport::default()),
        };
        if let Some(at) = report.first_sample_at {
            timeline.mark(Mark::FirstSample, at);
        }
        if let Some(at) = report.first_sound_at {
            timeline.mark(Mark::FirstSound, at);
        }
        // Nothing is transcribed yet, so the dictation is finished as soon
        // as its audio has been captured.
        self.control.processing_finished();
        Some(FinishedDictation {
            summary: timeline.summary(),
            stopped_because,
            audio,
        })
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
                self.phase = Phase::StoppingCapture(timeline, stopped_because);
                Some(CaptureCommand::Stop)
            }
            Decision::Ignore => None,
        }
    }
}
