use crate::DictationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PressId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    PushToTalk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Start(DictationId),
    Stop(DictationId),
    Ignore,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    Recording(DictationId),
    Processing,
}

#[derive(Debug)]
pub struct RecordingControl {
    state: State,
    dictations_started: u64,
}

impl RecordingControl {
    pub fn new(_mode: Mode) -> Self {
        Self {
            state: State::Idle,
            dictations_started: 0,
        }
    }

    pub fn key_pressed(&mut self, _press: PressId) -> Decision {
        let State::Idle = self.state else {
            return Decision::Ignore;
        };
        self.dictations_started += 1;
        let dictation = DictationId(self.dictations_started);
        self.state = State::Recording(dictation);
        Decision::Start(dictation)
    }

    pub fn key_released(&mut self, _press: PressId) -> Decision {
        let State::Recording(dictation) = self.state else {
            return Decision::Ignore;
        };
        self.state = State::Processing;
        Decision::Stop(dictation)
    }

    pub fn processing_finished(&mut self) {
        if let State::Processing = self.state {
            self.state = State::Idle;
        }
    }
}
