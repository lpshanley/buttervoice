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
    Manual,
    KeyListenerLost,
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
        mode: Mode,
    },
    /// Tapped once: still capturing, in case a second tap makes it hands-free.
    AwaitingSecondTap {
        dictation: DictationId,
    },
    Processing,
}

#[derive(Debug)]
pub struct RecordingControl {
    mode_for_next_dictation: Mode,
    state: State,
    dictations_started: u64,
}

impl RecordingControl {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode_for_next_dictation: mode,
            state: State::Idle,
            dictations_started: 0,
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode_for_next_dictation = mode;
    }

    pub fn latest_dictation(&self) -> Option<DictationId> {
        (self.dictations_started > 0).then_some(DictationId(self.dictations_started))
    }

    pub fn key_pressed(&mut self, press: PressId) -> Decision {
        match self.state {
            State::Idle => self.start(press),
            State::Recording {
                dictation,
                started_by,
                mode: Mode::Toggle,
            } if press != started_by => self.stop(dictation, StopReason::KeyPressedAgain),
            State::AwaitingSecondTap { dictation } => {
                self.state = State::Recording {
                    dictation,
                    started_by: press,
                    mode: Mode::Toggle,
                };
                Decision::Ignore
            }
            _ => Decision::Ignore,
        }
    }

    /// A press let go too soon to have been held for speech. In push-to-talk
    /// the dictation goes on capturing, waiting for a second tap.
    pub fn key_tapped(&mut self, press: PressId) -> Decision {
        match self.state {
            State::Recording {
                dictation,
                started_by,
                mode: Mode::PushToTalk,
            } if press == started_by => {
                self.state = State::AwaitingSecondTap { dictation };
                Decision::Ignore
            }
            _ => self.key_released(press),
        }
    }

    pub fn awaiting_second_tap(&self) -> bool {
        matches!(self.state, State::AwaitingSecondTap { .. })
    }

    pub fn second_tap_missed(&mut self) -> Decision {
        match self.state {
            State::AwaitingSecondTap { dictation } => self.stop(dictation, StopReason::KeyReleased),
            _ => Decision::Ignore,
        }
    }

    pub fn key_released(&mut self, press: PressId) -> Decision {
        match self.state {
            State::Recording {
                dictation,
                started_by,
                mode: Mode::PushToTalk,
            } if press == started_by => self.stop(dictation, StopReason::KeyReleased),
            _ => Decision::Ignore,
        }
    }

    pub fn manual_stop(&mut self) -> Decision {
        self.stop_any_recording(StopReason::Manual)
    }

    pub fn key_listener_lost(&mut self) -> Decision {
        self.stop_any_recording(StopReason::KeyListenerLost)
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
            mode: self.mode_for_next_dictation,
        };
        Decision::Start(dictation)
    }

    fn stop_any_recording(&mut self, reason: StopReason) -> Decision {
        match self.state {
            State::Recording { dictation, .. } | State::AwaitingSecondTap { dictation } => {
                self.stop(dictation, reason)
            }
            _ => Decision::Ignore,
        }
    }

    fn stop(&mut self, dictation: DictationId, reason: StopReason) -> Decision {
        self.state = State::Processing;
        Decision::Stop(dictation, reason)
    }
}
