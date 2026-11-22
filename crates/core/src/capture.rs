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
