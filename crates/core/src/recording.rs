use crate::DictationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PressId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    PushToTalk,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    KeyReleased,
    KeyPressedAgain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Start(DictationId),
    Stop(DictationId, StopReason),
    Ignore,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    Recording {
        dictation: DictationId,
        started_by: PressId,
    },
    Processing,
}

#[derive(Debug)]
pub struct RecordingControl {
    mode: Mode,
    state: State,
    dictations_started: u64,
}

impl RecordingControl {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            state: State::Idle,
            dictations_started: 0,
        }
    }

    pub fn key_pressed(&mut self, press: PressId) -> Decision {
        match (self.state, self.mode) {
            (State::Idle, _) => self.start(press),
            (
                State::Recording {
                    dictation,
                    started_by,
                },
                Mode::Toggle,
            ) if press != started_by => self.stop(dictation, StopReason::KeyPressedAgain),
            _ => Decision::Ignore,
        }
    }

    pub fn key_released(&mut self, press: PressId) -> Decision {
        match (self.state, self.mode) {
            (
                State::Recording {
                    dictation,
                    started_by,
                },
                Mode::PushToTalk,
            ) if press == started_by => self.stop(dictation, StopReason::KeyReleased),
            _ => Decision::Ignore,
        }
    }

    pub fn processing_finished(&mut self) {
        if let State::Processing = self.state {
            self.state = State::Idle;
        }
    }

    fn start(&mut self, press: PressId) -> Decision {
        self.dictations_started += 1;
        let dictation = DictationId(self.dictations_started);
        self.state = State::Recording {
            dictation,
            started_by: press,
        };
        Decision::Start(dictation)
    }

    fn stop(&mut self, dictation: DictationId, reason: StopReason) -> Decision {
        self.state = State::Processing;
        Decision::Stop(dictation, reason)
    }
}
