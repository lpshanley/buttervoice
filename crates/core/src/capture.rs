use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::timeline::Timestamp;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureReport {
    pub first_sample_at: Option<Timestamp>,
    pub first_sound_at: Option<Timestamp>,
}

/// Written from the real-time audio thread, so it only ever touches atomics:
/// no locks, no allocation.
#[derive(Debug, Default)]
struct CaptureMarks {
    first_sample_nanos: AtomicU64,
    first_sound_nanos: AtomicU64,
}

const UNSET: u64 = 0;

impl CaptureMarks {
    fn buffer_captured(&self, samples: &[f32], captured_at: Timestamp) {
        set_once(&self.first_sample_nanos, captured_at);
        if samples.iter().any(|sample| *sample != 0.0) {
            set_once(&self.first_sound_nanos, captured_at);
        }
    }

    fn report(&self) -> CaptureReport {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// `samples` holds the channels interleaved, as the device delivered them.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedAudio {
    pub format: AudioFormat,
    pub samples: Vec<f32>,
    pub dropped_samples: u64,
}

/// `peak` and `rms` are on the sample scale, where 1.0 is full scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioStats {
    pub format: AudioFormat,
    pub duration: Duration,
    pub dropped_samples: u64,
    pub peak: f32,
    pub rms: f32,
}

/// Audio in this format could not be converted to 16 kHz mono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversionFailed(pub AudioFormat);

/// The only rate speech recognition models take.
const SPEECH_SAMPLE_RATE: u32 = 16_000;

fn resampled_for_speech(mono: Vec<f32>, sample_rate: u32) -> Option<Vec<f32>> {
    if sample_rate == SPEECH_SAMPLE_RATE {
        return Some(mono);
    }
    let frames = mono.len();
    let mut resampler = Fft::<f32>::new(
        sample_rate as usize,
        SPEECH_SAMPLE_RATE as usize,
        RESAMPLER_CHUNK_FRAMES,
        1,
        FixedSync::Both,
    )
    .ok()?;
    let input = InterleavedSlice::new(&mono, 1, frames).ok()?;
    let output = resampler.process_all(&input, frames, None).ok()?;
    Some(output.take_data())
}

const RESAMPLER_CHUNK_FRAMES: usize = 1024;

/// -3 dBFS, as a sample value.
const TRANSCRIPTION_PEAK: f32 = 0.708;

#[derive(Debug, Clone, PartialEq)]
pub struct Captured {
    pub audio: CapturedAudio,
    pub report: CaptureReport,
    pub input: InputKind,
}

impl CapturedAudio {
    /// A 16-bit PCM WAV file, held in memory.
    pub fn to_wav(&self) -> Vec<u8> {
        const BYTES_PER_SAMPLE: u32 = 2;
        const HEADER_BYTES_AFTER_RIFF_SIZE: u32 = 36;
        let channels = self.format.channels;
        let bytes_per_frame = u32::from(channels) * BYTES_PER_SAMPLE;
        let data_bytes = self.samples.len() as u32 * BYTES_PER_SAMPLE;

        let mut wav = Vec::with_capacity(44 + data_bytes as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(HEADER_BYTES_AFTER_RIFF_SIZE + data_bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&self.format.sample_rate.to_le_bytes());
        wav.extend_from_slice(&(self.format.sample_rate * bytes_per_frame).to_le_bytes());
        wav.extend_from_slice(&(bytes_per_frame as u16).to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for sample in &self.samples {
            let pcm = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            wav.extend_from_slice(&pcm.to_le_bytes());
        }
        wav
    }

    /// The audio as it should be sent for transcription. A microphone across
    /// the desk captures speech some 25 dB below full scale, and replaying
    /// recordings showed fewer word errors once it was raised.
    pub fn boosted_for_transcription(&self) -> CapturedAudio {
        self.with_peak_at(TRANSCRIPTION_PEAK)
    }

    /// The same audio, scaled so its loudest sample is at `peak`.
    pub fn with_peak_at(&self, peak: f32) -> CapturedAudio {
        let current = self.stats().peak;
        if current == 0.0 {
            return self.clone();
        }
        let gain = peak / current;
        CapturedAudio {
            samples: self.samples.iter().map(|sample| sample * gain).collect(),
            ..self.clone()
        }
    }

    pub fn to_16khz_mono(&self) -> Result<CapturedAudio, ConversionFailed> {
        let failed = ConversionFailed(self.format);
        let channels = usize::from(self.format.channels);
        if channels == 0 || self.format.sample_rate == 0 {
            return Err(failed);
        }
        let mono: Vec<f32> = self
            .samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
        Ok(CapturedAudio {
            format: AudioFormat {
                sample_rate: SPEECH_SAMPLE_RATE,
                channels: 1,
            },
            samples: resampled_for_speech(mono, self.format.sample_rate).ok_or(failed)?,
            dropped_samples: self.dropped_samples,
        })
    }

    pub fn stats(&self) -> AudioStats {
        AudioStats {
            format: self.format,
            duration: self.duration(),
            dropped_samples: self.dropped_samples,
            peak: self
                .samples
                .iter()
                .fold(0.0, |peak, sample| peak.max(sample.abs())),
            rms: self.rms(),
        }
    }

    fn rms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum_of_squares: f64 = self
            .samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum();
        (sum_of_squares / self.samples.len() as f64).sqrt() as f32
    }

    pub fn duration(&self) -> Duration {
        let frames = self.samples.len() as u64 / u64::from(self.format.channels);
        Duration::from_nanos(frames * 1_000_000_000 / u64::from(self.format.sample_rate))
    }
}

#[derive(Debug, Default)]
struct Shared {
    marks: CaptureMarks,
    dropped_samples: AtomicU64,
}

/// The audio thread's end of the channel. Writing never locks or allocates.
pub struct CaptureWriter {
    samples: rtrb::Producer<f32>,
    shared: Arc<Shared>,
}

pub struct CaptureReader {
    input: InputKind,
    format: AudioFormat,
    samples: rtrb::Consumer<f32>,
    collected: Vec<f32>,
    shared: Arc<Shared>,
}

/// `buffered` is how much audio may wait between two calls to
/// [`CaptureReader::collect`] before samples are dropped.
pub fn capture_channel(
    input: InputKind,
    format: AudioFormat,
    buffered: Duration,
) -> (CaptureWriter, CaptureReader) {
    let samples_per_second = u128::from(format.sample_rate) * u128::from(format.channels);
    let capacity = (samples_per_second * buffered.as_nanos() / 1_000_000_000) as usize;
    let (producer, consumer) = rtrb::RingBuffer::new(capacity);
    let shared = Arc::new(Shared::default());
    (
        CaptureWriter {
            samples: producer,
            shared: shared.clone(),
        },
        CaptureReader {
            input,
            format,
            samples: consumer,
            collected: Vec::new(),
            shared,
        },
    )
}

impl CaptureWriter {
    pub fn buffer_captured(&mut self, samples: &[f32], captured_at: Timestamp) {
        self.shared.marks.buffer_captured(samples, captured_at);
        let (_, did_not_fit) = self.samples.push_partial_slice(samples);
        if !did_not_fit.is_empty() {
            self.shared
                .dropped_samples
                .fetch_add(did_not_fit.len() as u64, Relaxed);
        }
    }
}

impl CaptureReader {
    pub fn collect(&mut self) {
        if let Ok(waiting) = self.samples.read_chunk(self.samples.slots()) {
            let (first, second) = waiting.as_slices();
            self.collected.extend_from_slice(first);
            self.collected.extend_from_slice(second);
            waiting.commit_all();
        }
    }

    pub fn finish(mut self) -> Captured {
        self.collect();
        Captured {
            audio: CapturedAudio {
                format: self.format,
                samples: self.collected,
                dropped_samples: self.shared.dropped_samples.load(Relaxed),
            },
            report: self.shared.marks.report(),
            input: self.input,
        }
    }
}

const BUILT_IN_MICROPHONE: &str = "BuiltInMicrophoneDevice";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    BuiltIn,
    Other,
}

pub fn input_kind(id: &str) -> InputKind {
    if id == BUILT_IN_MICROPHONE {
        InputKind::BuiltIn
    } else {
        InputKind::Other
    }
}

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
