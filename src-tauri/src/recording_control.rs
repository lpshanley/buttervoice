//! Recording ownership and stop arbitration, independent of audio and UI work.

use std::time::Instant;

use crate::app_state::DictationState;
use crate::hotkey_macos::{DictationMode, HotkeyBinding, HotkeyConfig, HotkeyKey};

#[derive(Debug, Clone, Copy)]
pub(crate) enum RecordingSource {
    Manual,
    Hotkey {
        binding: HotkeyBinding,
        press_id: u64,
    },
}

#[derive(Debug)]
pub(crate) struct RecordingSession {
    pub source: RecordingSource,
    pub trace_id: String,
    pub started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum StopRequest {
    Manual,
    Release {
        binding: HotkeyBinding,
        press_id: u64,
    },
    TogglePress {
        binding: HotkeyBinding,
        press_id: u64,
    },
    Interrupted {
        revision: u64,
        through_press_id: u64,
    },
}

pub(crate) struct RecordingControl {
    pub state: DictationState,
    pub session: Option<RecordingSession>,
}

impl Default for RecordingControl {
    fn default() -> Self {
        Self {
            state: DictationState::Idle,
            session: None,
        }
    }
}

impl RecordingControl {
    /// Keep the original binding usable for the whole recording, including
    /// the second press that ends a toggle recording.
    pub fn configure_hotkey(
        &self,
        config: &HotkeyConfig,
        key: HotkeyKey,
        mode: DictationMode,
    ) -> bool {
        if self.session.is_some() {
            return false;
        }
        let current = config.snapshot();
        if current.key != key || current.mode != mode {
            config.update(&key, mode);
        }
        true
    }

    pub fn start(
        &mut self,
        source: RecordingSource,
        current_binding: HotkeyBinding,
        trace_id: String,
    ) -> bool {
        if let RecordingSource::Hotkey { binding, .. } = source {
            if binding != current_binding {
                return false;
            }
        }
        if !self.state.try_start_recording() {
            return false;
        }
        self.session = Some(RecordingSession {
            source,
            trace_id,
            started_at: Instant::now(),
        });
        true
    }

    pub fn stop(&mut self, request: StopRequest) -> Option<RecordingSession> {
        if !matches!(self.state, DictationState::Recording) {
            return None;
        }
        let session = self.session.as_ref()?;
        let accepted = match (request, session.source) {
            (StopRequest::Manual, _) => true,
            (
                StopRequest::Release { binding, press_id },
                RecordingSource::Hotkey {
                    binding: original,
                    press_id: started,
                },
            ) => {
                original.mode == DictationMode::PushToTalk
                    && binding == original
                    && press_id == started
            }
            (
                StopRequest::TogglePress { binding, press_id },
                RecordingSource::Hotkey {
                    binding: original,
                    press_id: started,
                },
            ) => {
                original.mode == DictationMode::Toggle && binding == original && press_id > started
            }
            (
                StopRequest::Interrupted {
                    revision,
                    through_press_id,
                },
                RecordingSource::Hotkey { binding, press_id },
            ) => binding.revision == revision && press_id <= through_press_id,
            _ => false,
        };
        if !accepted {
            return None;
        }
        self.state = DictationState::Transcribing;
        self.session.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey_macos::HotkeyKey;
    use parking_lot::Mutex;
    use std::sync::{Arc, Barrier};

    fn binding(mode: DictationMode, revision: u64) -> HotkeyBinding {
        HotkeyBinding {
            key: HotkeyKey::RightOption,
            mode,
            revision,
        }
    }

    fn start(control: &mut RecordingControl, binding: HotkeyBinding, press_id: u64) {
        assert!(control.start(
            RecordingSource::Hotkey { binding, press_id },
            binding,
            format!("session-{press_id}")
        ));
    }

    #[test]
    fn original_push_to_talk_release_stops_once() {
        let mut control = RecordingControl::default();
        let original = binding(DictationMode::PushToTalk, 1);
        start(&mut control, original, 10);
        let release = StopRequest::Release {
            binding: original,
            press_id: 10,
        };
        assert_eq!(control.stop(release).unwrap().trace_id, "session-10");
        assert!(control.stop(release).is_none());
        assert!(!control.start(RecordingSource::Manual, original, "too-early".into()));
    }

    #[test]
    fn delayed_release_and_interruption_cannot_stop_new_session() {
        let mut control = RecordingControl::default();
        let key = binding(DictationMode::PushToTalk, 1);
        start(&mut control, key, 10);
        assert!(control.stop(StopRequest::Manual).is_some());
        control.state = DictationState::Idle;
        start(&mut control, key, 20);
        assert!(control
            .stop(StopRequest::Release {
                binding: key,
                press_id: 10
            })
            .is_none());
        assert!(control
            .stop(StopRequest::Interrupted {
                revision: 1,
                through_press_id: 10
            })
            .is_none());
        assert_eq!(control.session.as_ref().unwrap().trace_id, "session-20");
    }

    #[test]
    fn toggle_needs_a_new_press_and_ignores_release() {
        let mut control = RecordingControl::default();
        let key = binding(DictationMode::Toggle, 1);
        start(&mut control, key, 10);
        assert!(control
            .stop(StopRequest::Release {
                binding: key,
                press_id: 10
            })
            .is_none());
        assert!(control
            .stop(StopRequest::TogglePress {
                binding: key,
                press_id: 10
            })
            .is_none());
        assert!(control
            .stop(StopRequest::TogglePress {
                binding: key,
                press_id: 11
            })
            .is_some());
    }

    #[test]
    fn interruption_stops_toggle_but_not_manual_recording() {
        let mut control = RecordingControl::default();
        let key = binding(DictationMode::Toggle, 1);
        let lost = StopRequest::Interrupted {
            revision: 1,
            through_press_id: 10,
        };
        start(&mut control, key, 10);
        assert!(control.stop(lost).is_some());
        control.state = DictationState::Idle;
        assert!(control.start(RecordingSource::Manual, key, "manual".into()));
        assert!(control.stop(lost).is_none());
        assert!(control
            .stop(StopRequest::Release {
                binding: key,
                press_id: 10
            })
            .is_none());
        assert!(control.stop(StopRequest::Manual).is_some());
    }

    #[test]
    fn stale_configuration_cannot_start_or_stop_recording() {
        let mut control = RecordingControl::default();
        let old = binding(DictationMode::PushToTalk, 1);
        let new = binding(DictationMode::Toggle, 2);
        assert!(!control.start(
            RecordingSource::Hotkey {
                binding: old,
                press_id: 1
            },
            new,
            "stale".into()
        ));
        start(&mut control, new, 2);
        assert!(control
            .stop(StopRequest::Interrupted {
                revision: 1,
                through_press_id: 100
            })
            .is_none());
        assert!(control
            .stop(StopRequest::Release {
                binding: old,
                press_id: 2
            })
            .is_none());
    }

    #[test]
    fn configuration_changes_wait_for_original_release_and_coalesce() {
        let mut control = RecordingControl::default();
        let config = HotkeyConfig::new(&HotkeyKey::RightOption, DictationMode::PushToTalk);
        let original = config.snapshot();
        start(&mut control, original, 10);
        assert!(!control.configure_hotkey(&config, HotkeyKey::RightControl, DictationMode::Toggle));
        assert!(!control.configure_hotkey(&config, HotkeyKey::Fn, DictationMode::Toggle));
        assert_eq!(config.snapshot(), original);
        assert!(control
            .stop(StopRequest::Release {
                binding: original,
                press_id: 10
            })
            .is_some());
        assert!(control.configure_hotkey(&config, HotkeyKey::Fn, DictationMode::Toggle));
        let updated = config.snapshot();
        assert_eq!(updated.key, HotkeyKey::Fn);
        assert_eq!(updated.mode, DictationMode::Toggle);
        assert_ne!(updated.revision, original.revision);
    }

    #[test]
    fn changing_toggle_to_hold_preserves_original_stop_press() {
        let mut control = RecordingControl::default();
        let config = HotkeyConfig::new(&HotkeyKey::RightOption, DictationMode::Toggle);
        let original = config.snapshot();
        start(&mut control, original, 10);
        assert!(!control.configure_hotkey(&config, HotkeyKey::Fn, DictationMode::PushToTalk));
        assert!(control
            .stop(StopRequest::TogglePress {
                binding: original,
                press_id: 11
            })
            .is_some());
        assert!(control.configure_hotkey(&config, HotkeyKey::Fn, DictationMode::PushToTalk));
    }

    #[test]
    fn racing_manual_and_automatic_stops_claim_one_session() {
        let key = binding(DictationMode::PushToTalk, 1);
        let mut control = RecordingControl::default();
        start(&mut control, key, 10);
        let control = Arc::new(Mutex::new(control));
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [
            StopRequest::Manual,
            StopRequest::Release {
                binding: key,
                press_id: 10,
            },
        ]
        .into_iter()
        .map(|request| {
            let control = control.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                control.lock().stop(request).is_some()
            })
        })
        .collect();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| usize::from(h.join().unwrap()))
                .sum::<usize>(),
            1
        );
    }
}
