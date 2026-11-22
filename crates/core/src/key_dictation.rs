use crate::hotkey::{HotkeyDecoder, HotkeyKey, KeyEdge};
use crate::recording::{Decision, Mode, RecordingControl, StopReason};
use crate::timeline::{Mark, Summary, Timeline, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedDictation {
    pub summary: Summary,
    pub stopped_because: StopReason,
}

#[derive(Debug)]
pub struct KeyDictations {
    decoder: HotkeyDecoder,
    control: RecordingControl,
    in_progress: Option<Timeline>,
}

impl KeyDictations {
    pub fn new(key: HotkeyKey, mode: Mode) -> Self {
        Self {
            decoder: HotkeyDecoder::new(key),
            control: RecordingControl::new(mode),
            in_progress: None,
        }
    }

    pub fn flags_changed(&mut self, flags: u64, at: Timestamp) -> Option<FinishedDictation> {
        let edge = self.decoder.flags_changed(flags)?;
        self.key_edge(edge, at)
    }

    /// `flags` is the modifier state read directly from macOS after key
    /// events may have been missed.
    pub fn resync(&mut self, flags: u64, at: Timestamp) -> Option<FinishedDictation> {
        let edge = self.decoder.resync(flags)?;
        self.key_edge(edge, at)
    }

    pub fn key_listener_lost(&mut self, at: Timestamp) -> Option<FinishedDictation> {
        let decision = self.control.key_listener_lost();
        self.apply(decision, at)
    }

    fn key_edge(&mut self, edge: KeyEdge, at: Timestamp) -> Option<FinishedDictation> {
        let decision = match edge {
            KeyEdge::Pressed(press) => self.control.key_pressed(press),
            KeyEdge::Released(press) => self.control.key_released(press),
        };
        self.apply(decision, at)
    }

    fn apply(&mut self, decision: Decision, at: Timestamp) -> Option<FinishedDictation> {
        match decision {
            Decision::Start(dictation) => {
                let mut timeline = Timeline::new(dictation);
                timeline.mark(Mark::KeyDown, at);
                self.in_progress = Some(timeline);
                None
            }
            Decision::Stop(_, stopped_because) => {
                let mut timeline = self.in_progress.take()?;
                timeline.mark(Mark::KeyUp, at);
                // Nothing is captured or transcribed yet, so a stopped
                // dictation is finished at once.
                self.control.processing_finished();
                Some(FinishedDictation {
                    summary: timeline.summary(),
                    stopped_because,
                })
            }
            Decision::Ignore => None,
        }
    }
}
