use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use crate::timeline::Timestamp;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureReport {
    pub first_sample_at: Option<Timestamp>,
    pub first_sound_at: Option<Timestamp>,
}

/// Written from the real-time audio thread, so it only ever touches atomics:
/// no locks, no allocation.
#[derive(Debug, Default)]
pub struct CaptureMarks {
    first_sample_nanos: AtomicU64,
    first_sound_nanos: AtomicU64,
}

const UNSET: u64 = 0;

impl CaptureMarks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn buffer_captured(&self, samples: &[f32], captured_at: Timestamp) {
        set_once(&self.first_sample_nanos, captured_at);
        if samples.iter().any(|sample| *sample != 0.0) {
            set_once(&self.first_sound_nanos, captured_at);
        }
    }

    pub fn report(&self) -> CaptureReport {
        CaptureReport {
            first_sample_at: read(&self.first_sample_nanos),
            first_sound_at: read(&self.first_sound_nanos),
        }
    }
}

fn set_once(mark: &AtomicU64, at: Timestamp) {
    let _ = mark.compare_exchange(UNSET, at.as_nanos(), Relaxed, Relaxed);
}

fn read(mark: &AtomicU64) -> Option<Timestamp> {
    match mark.load(Relaxed) {
        UNSET => None,
        nanos => Some(Timestamp::from_nanos(nanos)),
    }
}

const BUILT_IN_MICROPHONE: &str = "BuiltInMicrophoneDevice";

/// The input device to capture from, out of the ids macOS lists. The Mac's
/// own microphone wins over the system default because a Bluetooth headset
/// delivers most of a second of silence while it switches into call mode.
pub fn choose_input<'a>(available: &[&'a str], system_default: Option<&'a str>) -> Option<&'a str> {
    available
        .iter()
        .copied()
        .find(|id| *id == BUILT_IN_MICROPHONE)
        .or(system_default)
}
