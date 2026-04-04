use std::collections::VecDeque;
use std::f32::consts::PI;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, select, unbounded, Receiver, Sender, TrySendError};
use parking_lot::Mutex;
use rubato::{
    Resampler as RubatoResampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use serde::{Deserialize, Serialize};

use crate::settings::{AudioChannelMode, AudioQualityPreset, HighPassFilter};

const PERSISTENT_PREROLL_MS: u64 = 600;
const MAX_INPUT_GAIN_DB: f32 = 24.0;
const WHISPER_SAMPLE_RATE: u32 = 16_000;
const RESAMPLER_CHUNK_SIZE: usize = 1024;
const STAGING_CHANNEL_CAPACITY: usize = 128;
const I16_SCALE: f32 = 32_768.0;

type SharedError = Arc<Mutex<Option<String>>>;
type ControlResult<T> = std::result::Result<T, String>;

fn output_sample_rate(source_rate: u32) -> u32 {
    if source_rate > WHISPER_SAMPLE_RATE {
        WHISPER_SAMPLE_RATE
    } else {
        source_rate
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicDevice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureTuning {
    pub audio_channel_mode: AudioChannelMode,
    pub input_gain_db: f32,
    pub high_pass_filter: HighPassFilter,
    pub audio_quality_preset: AudioQualityPreset,
}

impl CaptureTuning {
    fn sanitize(self) -> Self {
        Self {
            audio_channel_mode: self.audio_channel_mode,
            input_gain_db: self
                .input_gain_db
                .clamp(-MAX_INPUT_GAIN_DB, MAX_INPUT_GAIN_DB),
            high_pass_filter: self.high_pass_filter,
            audio_quality_preset: self.audio_quality_preset,
        }
    }
}

#[derive(Debug, Clone)]
struct HighPassFilterState {
    alpha: f32,
    previous_input: f32,
    previous_output: f32,
}

impl HighPassFilterState {
    fn new(sample_rate_hz: u32, cutoff_hz: f32) -> Option<Self> {
        if sample_rate_hz == 0 || cutoff_hz <= 0.0 {
            return None;
        }

        let dt = 1.0 / sample_rate_hz as f32;
        let rc = 1.0 / (2.0 * PI * cutoff_hz);
        let alpha = rc / (rc + dt);
        Some(Self {
            alpha,
            previous_input: 0.0,
            previous_output: 0.0,
        })
    }

    fn process(&mut self, sample: f32) -> f32 {
        let output = self.alpha * (self.previous_output + sample - self.previous_input);
        self.previous_input = sample;
        self.previous_output = output;
        output
    }
}

#[derive(Debug, Clone)]
struct SampleProcessor {
    audio_channel_mode: AudioChannelMode,
    gain_multiplier: f32,
    high_pass_filter: Option<HighPassFilterState>,
}

impl SampleProcessor {
    fn new(sample_rate_hz: u32, tuning: CaptureTuning) -> Self {
        let sanitized = tuning.sanitize();
        let gain_multiplier = 10.0_f32.powf(sanitized.input_gain_db / 20.0);
        let high_pass_filter = sanitized
            .high_pass_filter
            .cutoff_hz()
            .and_then(|cutoff_hz| HighPassFilterState::new(sample_rate_hz, cutoff_hz));

        Self {
            audio_channel_mode: sanitized.audio_channel_mode,
            gain_multiplier,
            high_pass_filter,
        }
    }

    fn process_i16_frame(&mut self, frame: &[i16]) -> i16 {
        let sample = select_i16_frame(frame, self.audio_channel_mode);
        self.apply_processing(sample)
    }

    fn process_u16_frame(&mut self, frame: &[u16]) -> i16 {
        let sample = select_u16_frame(frame, self.audio_channel_mode);
        self.apply_processing(sample)
    }

    fn process_f32_frame(&mut self, frame: &[f32]) -> i16 {
        let sample = select_f32_frame(frame, self.audio_channel_mode);
        self.apply_processing(sample)
    }

    fn apply_processing(&mut self, sample: i16) -> i16 {
        let mut processed = sample as f32;
        if let Some(filter) = self.high_pass_filter.as_mut() {
            processed = filter.process(processed);
        }
        processed *= self.gain_multiplier;
        processed.clamp(i16::MIN as f32, i16::MAX as f32).round() as i16
    }
}

#[derive(Debug, Clone, Copy)]
struct AudioQualityProfile {
    sinc_len: usize,
    cutoff: f32,
    oversampling_factor: usize,
}

impl AudioQualityPreset {
    fn profile(self) -> AudioQualityProfile {
        match self {
            Self::Balanced => AudioQualityProfile {
                sinc_len: 256,
                cutoff: 0.95,
                oversampling_factor: 256,
            },
            Self::BestAccuracy => AudioQualityProfile {
                sinc_len: 512,
                cutoff: 0.947,
                oversampling_factor: 256,
            },
            Self::LowCpu => AudioQualityProfile {
                sinc_len: 128,
                cutoff: 0.93,
                oversampling_factor: 128,
            },
        }
    }
}

enum SpeechResampler {
    Passthrough,
    Rubato(RubatoSpeechResampler),
}

struct RubatoSpeechResampler {
    resampler: SincFixedIn<f32>,
    input_buffer: Vec<Vec<f32>>,
    output_buffer: Vec<Vec<f32>>,
    pending: VecDeque<f32>,
    delay_trim_remaining: usize,
    chunk_size: usize,
}

impl SpeechResampler {
    fn new(source_rate: u32, target_rate: u32, quality_preset: AudioQualityPreset) -> Result<Self> {
        if source_rate <= target_rate {
            return Ok(Self::Passthrough);
        }

        let profile = quality_preset.profile();
        let parameters = SincInterpolationParameters {
            sinc_len: profile.sinc_len,
            f_cutoff: profile.cutoff,
            oversampling_factor: profile.oversampling_factor,
            interpolation: SincInterpolationType::Cubic,
            window: WindowFunction::BlackmanHarris2,
        };

        let resampler = SincFixedIn::<f32>::new(
            target_rate as f64 / source_rate as f64,
            1.0,
            parameters,
            RESAMPLER_CHUNK_SIZE,
            1,
        )
        .map_err(|err| anyhow!("failed creating rubato resampler: {err}"))?;

        let input_buffer = resampler.input_buffer_allocate(true);
        let output_buffer = resampler.output_buffer_allocate(true);
        let delay_trim_remaining = resampler.output_delay();

        Ok(Self::Rubato(RubatoSpeechResampler {
            resampler,
            input_buffer,
            output_buffer,
            pending: VecDeque::with_capacity(RESAMPLER_CHUNK_SIZE * 2),
            delay_trim_remaining,
            chunk_size: RESAMPLER_CHUNK_SIZE,
        }))
    }

    fn process_batch(&mut self, input: &[i16], output: &mut Vec<i16>) -> Result<()> {
        match self {
            Self::Passthrough => {
                output.extend_from_slice(input);
                Ok(())
            }
            Self::Rubato(state) => state.process_batch(input, output),
        }
    }

    fn flush(&mut self, output: &mut Vec<i16>) -> Result<()> {
        match self {
            Self::Passthrough => Ok(()),
            Self::Rubato(state) => state.flush(output),
        }
    }
}

impl RubatoSpeechResampler {
    fn process_batch(&mut self, input: &[i16], output: &mut Vec<i16>) -> Result<()> {
        self.pending
            .extend(input.iter().map(|sample| *sample as f32 / I16_SCALE));

        while self.pending.len() >= self.chunk_size {
            let (front, back) = self.pending.as_slices();
            let front_take = front.len().min(self.chunk_size);
            self.input_buffer[0][..front_take].copy_from_slice(&front[..front_take]);
            if front_take < self.chunk_size {
                let remaining = self.chunk_size - front_take;
                self.input_buffer[0][front_take..self.chunk_size]
                    .copy_from_slice(&back[..remaining]);
            }
            self.pending.drain(..self.chunk_size);

            let (_, written) = self
                .resampler
                .process_into_buffer(&self.input_buffer, &mut self.output_buffer, None)
                .map_err(|err| anyhow!("failed resampling audio chunk: {err}"))?;
            let chunk = self.output_buffer[0][..written].to_vec();
            self.collect_output(&chunk, output);
        }

        Ok(())
    }

    fn flush(&mut self, output: &mut Vec<i16>) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }

        let partial = self.pending.drain(..).collect::<Vec<_>>();
        let partial_input = vec![partial];
        let flushed = self
            .resampler
            .process_partial(Some(&partial_input), None)
            .map_err(|err| anyhow!("failed flushing resampler tail: {err}"))?;
        if let Some(channel) = flushed.first() {
            self.collect_output(channel, output);
        }

        Ok(())
    }

    fn collect_output(&mut self, input: &[f32], output: &mut Vec<i16>) {
        let skip = self.delay_trim_remaining.min(input.len());
        self.delay_trim_remaining -= skip;
        output.extend(input[skip..].iter().map(|sample| f32_to_i16(*sample)));
    }
}

enum ProcessorCommand {
    StartWriter {
        writer: hound::WavWriter<BufWriter<File>>,
        response: Sender<ControlResult<()>>,
    },
    StopWriter {
        response: Sender<ControlResult<u64>>,
    },
    Shutdown {
        response: Sender<ControlResult<()>>,
    },
}

struct ProcessorHandle {
    sample_tx: Sender<Vec<i16>>,
    command_tx: Sender<ProcessorCommand>,
    fatal_error: SharedError,
    overflow_count: Arc<AtomicU64>,
    join_handle: Option<JoinHandle<()>>,
}

impl ProcessorHandle {
    fn sample_tx(&self) -> Sender<Vec<i16>> {
        self.sample_tx.clone()
    }

    fn overflow_count(&self) -> Arc<AtomicU64> {
        self.overflow_count.clone()
    }

    fn start_writer(&self, writer: hound::WavWriter<BufWriter<File>>) -> Result<()> {
        let (response_tx, response_rx) = bounded(1);
        self.command_tx
            .send(ProcessorCommand::StartWriter {
                writer,
                response: response_tx,
            })
            .context("failed sending start-writer command to audio processor")?;
        response_rx
            .recv()
            .context("audio processor disconnected while starting writer")?
            .map_err(|err| anyhow!(err))
    }

    fn stop_writer(&self) -> Result<u64> {
        let (response_tx, response_rx) = bounded(1);
        self.command_tx
            .send(ProcessorCommand::StopWriter {
                response: response_tx,
            })
            .context("failed sending stop-writer command to audio processor")?;
        response_rx
            .recv()
            .context("audio processor disconnected while stopping writer")?
            .map_err(|err| anyhow!(err))
    }

    fn shutdown(mut self) -> Result<()> {
        let (response_tx, response_rx) = bounded(1);
        self.command_tx
            .send(ProcessorCommand::Shutdown {
                response: response_tx,
            })
            .context("failed sending shutdown command to audio processor")?;
        drop(self.sample_tx);
        let result = response_rx
            .recv()
            .context("audio processor disconnected while shutting down")?
            .map_err(|err| anyhow!(err));

        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }

        result
    }
}

struct ProcessorState {
    resampler: SpeechResampler,
    ring_buffer: Option<VecDeque<i16>>,
    max_buffer_samples: usize,
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    fatal_error: SharedError,
    overflow_count: Arc<AtomicU64>,
    flush_on_shutdown: bool,
}

impl ProcessorState {
    fn new(
        source_rate_hz: u32,
        quality_preset: AudioQualityPreset,
        max_buffer_samples: usize,
        initial_writer: Option<hound::WavWriter<BufWriter<File>>>,
        flush_on_shutdown: bool,
        fatal_error: SharedError,
        overflow_count: Arc<AtomicU64>,
    ) -> Result<Self> {
        let resampler = SpeechResampler::new(source_rate_hz, WHISPER_SAMPLE_RATE, quality_preset)?;
        let ring_buffer = if max_buffer_samples > 0 {
            Some(VecDeque::with_capacity(max_buffer_samples + 1))
        } else {
            None
        };

        Ok(Self {
            resampler,
            ring_buffer,
            max_buffer_samples,
            writer: initial_writer,
            fatal_error,
            overflow_count,
            flush_on_shutdown,
        })
    }

    fn process_samples(&mut self, samples: &[i16]) -> Result<()> {
        let mut output = Vec::new();
        self.resampler.process_batch(samples, &mut output)?;
        self.write_output(&output)
    }

    fn start_writer(&mut self, mut writer: hound::WavWriter<BufWriter<File>>) -> Result<()> {
        if shared_error_message(&self.fatal_error).is_some() {
            return Err(anyhow!(
                "{}",
                shared_error_message(&self.fatal_error)
                    .unwrap_or_else(|| { "audio processor is in a failed state".to_string() })
            ));
        }

        if self.writer.is_some() {
            return Err(anyhow!("recording already in progress"));
        }

        if let Some(ring_buffer) = &self.ring_buffer {
            for sample in ring_buffer {
                writer
                    .write_sample(*sample)
                    .context("failed writing preroll audio")?;
            }
        }

        self.writer = Some(writer);
        Ok(())
    }

    fn stop_writer(&mut self) -> Result<u64> {
        let writer = self
            .writer
            .take()
            .ok_or_else(|| anyhow!("stop_recording called without active recording"))?;
        writer.finalize().context("failed finalizing wav file")?;
        let dropped = self.overflow_count.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            eprintln!("warning: dropped {dropped} audio batches due to processing backpressure");
        }
        self.error_if_fatal()?;
        Ok(dropped)
    }

    fn finish(&mut self) -> Result<()> {
        if self.flush_on_shutdown {
            let mut flushed = Vec::new();
            self.resampler.flush(&mut flushed)?;
            self.write_output(&flushed)?;
        }

        if let Some(writer) = self.writer.take() {
            writer.finalize().context("failed finalizing wav file")?;
        }

        self.error_if_fatal()
    }

    fn write_output(&mut self, samples: &[i16]) -> Result<()> {
        if let Some(ring_buffer) = self.ring_buffer.as_mut() {
            let overflow =
                (ring_buffer.len() + samples.len()).saturating_sub(self.max_buffer_samples);
            if overflow > 0 {
                let drain_count = overflow.min(ring_buffer.len());
                ring_buffer.drain(..drain_count);
            }
            ring_buffer.extend(samples.iter().copied());
            if ring_buffer.len() > self.max_buffer_samples {
                let excess = ring_buffer.len() - self.max_buffer_samples;
                ring_buffer.drain(..excess);
            }
        }

        if let Some(writer) = self.writer.as_mut() {
            for sample in samples {
                writer
                    .write_sample(*sample)
                    .context("failed writing audio sample")?;
            }
        }

        Ok(())
    }

    fn error_if_fatal(&self) -> Result<()> {
        if let Some(message) = shared_error_message(&self.fatal_error) {
            return Err(anyhow!(message));
        }
        Ok(())
    }
}

struct ActiveRecording {
    path: PathBuf,
    processor: ProcessorHandle,
    stream: cpal::Stream,
}

struct PersistentCapture {
    preferred_mic: Option<String>,
    capture_tuning: CaptureTuning,
    sample_rate_hz: u32,
    processor: ProcessorHandle,
    _stream: cpal::Stream,
    active_path: Option<PathBuf>,
}

enum AudioCommand {
    Start {
        out_dir: PathBuf,
        trace_id: String,
        preferred_mic: Option<String>,
        keep_mic_stream_open: bool,
        capture_tuning: CaptureTuning,
        response: Sender<ControlResult<PathBuf>>,
    },
    Stop {
        response: Sender<ControlResult<(PathBuf, u64)>>,
    },
    Configure {
        preferred_mic: Option<String>,
        keep_mic_stream_open: bool,
        capture_tuning: CaptureTuning,
        response: Sender<ControlResult<()>>,
    },
}

#[derive(Clone)]
pub struct AudioCapture {
    command_tx: Sender<AudioCommand>,
    input_level_peak: Arc<AtomicU32>,
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioCapture {
    pub fn new() -> Self {
        let (command_tx, command_rx) = unbounded::<AudioCommand>();
        let input_level_peak = Arc::new(AtomicU32::new(0));
        let input_level_peak_worker = input_level_peak.clone();

        std::thread::spawn(move || {
            let mut active: Option<ActiveRecording> = None;
            let mut persistent: Option<PersistentCapture> = None;

            while let Ok(command) = command_rx.recv() {
                match command {
                    AudioCommand::Start {
                        out_dir,
                        trace_id,
                        preferred_mic,
                        keep_mic_stream_open,
                        capture_tuning,
                        response,
                    } => {
                        input_level_peak_worker.store(0, Ordering::Relaxed);
                        if active.is_some() || has_persistent_recording(&persistent) {
                            let _ = response.send(Err("recording already in progress".to_string()));
                            continue;
                        }

                        if keep_mic_stream_open {
                            let result = ensure_persistent_capture(
                                &mut persistent,
                                preferred_mic.as_deref(),
                                capture_tuning,
                                input_level_peak_worker.clone(),
                            )
                            .and_then(|_| {
                                start_persistent_recording(persistent.as_mut(), &out_dir, &trace_id)
                            })
                            .map_err(|err| err.to_string());
                            let _ = response.send(result);
                            continue;
                        }

                        if let Some(existing) = persistent.take() {
                            let _ = shutdown_persistent_capture(existing);
                        }

                        match start_recording_inner(
                            &out_dir,
                            &trace_id,
                            preferred_mic.as_deref(),
                            capture_tuning,
                            input_level_peak_worker.clone(),
                        ) {
                            Ok(recording) => {
                                let path = recording.path.clone();
                                active = Some(recording);
                                let _ = response.send(Ok(path));
                            }
                            Err(err) => {
                                let _ = response.send(Err(err.to_string()));
                            }
                        }
                    }
                    AudioCommand::Stop { response } => {
                        if let Some(recording) = active.take() {
                            let stop_result =
                                stop_recording_inner(recording).map_err(|err| err.to_string());
                            if persistent.is_none() {
                                input_level_peak_worker.store(0, Ordering::Relaxed);
                            }
                            let _ = response.send(stop_result);
                            continue;
                        }

                        if has_persistent_recording(&persistent) {
                            let result = stop_persistent_recording(persistent.as_mut())
                                .map_err(|err| err.to_string());
                            let _ = response.send(result);
                            continue;
                        }

                        input_level_peak_worker.store(0, Ordering::Relaxed);
                        let _ = response.send(Err(
                            "stop_recording called without active recording".to_string(),
                        ));
                    }
                    AudioCommand::Configure {
                        preferred_mic,
                        keep_mic_stream_open,
                        capture_tuning,
                        response,
                    } => {
                        if active.is_some() || has_persistent_recording(&persistent) {
                            let _ = response
                                .send(Err("cannot reconfigure capture while recording is active"
                                    .to_string()));
                            continue;
                        }

                        let result = if keep_mic_stream_open {
                            ensure_persistent_capture(
                                &mut persistent,
                                preferred_mic.as_deref(),
                                capture_tuning,
                                input_level_peak_worker.clone(),
                            )
                        } else {
                            if let Some(existing) = persistent.take() {
                                if let Err(err) = shutdown_persistent_capture(existing) {
                                    let _ = response.send(Err(err.to_string()));
                                    continue;
                                }
                            }
                            input_level_peak_worker.store(0, Ordering::Relaxed);
                            Ok(())
                        };

                        let _ = response.send(result.map_err(|err| err.to_string()));
                    }
                }
            }

            if let Some(recording) = active.take() {
                let _ = stop_recording_inner(recording);
            }
            if let Some(capture) = persistent.take() {
                let _ = shutdown_persistent_capture(capture);
            }
        });

        Self {
            command_tx,
            input_level_peak,
        }
    }

    pub fn list_input_devices() -> Result<Vec<MicDevice>> {
        let host = cpal::default_host();
        let mut devices = Vec::new();

        for device in host
            .input_devices()
            .context("failed enumerating input devices")?
        {
            let name = device
                .name()
                .unwrap_or_else(|_| "Unknown microphone".to_string());
            devices.push(MicDevice {
                id: name.clone(),
                name,
            });
        }

        Ok(devices)
    }

    pub fn configure_capture(
        &self,
        preferred_mic: Option<&str>,
        keep_mic_stream_open: bool,
        capture_tuning: CaptureTuning,
    ) -> Result<()> {
        let (response_tx, response_rx) = bounded(1);
        self.command_tx
            .send(AudioCommand::Configure {
                preferred_mic: preferred_mic.map(ToOwned::to_owned),
                keep_mic_stream_open,
                capture_tuning: capture_tuning.sanitize(),
                response: response_tx,
            })
            .context("failed sending configure command to audio worker")?;

        let response = response_rx
            .recv()
            .context("audio worker disconnected while configuring capture")?;
        response.map_err(|err| anyhow!(err))
    }

    pub fn start_recording(
        &self,
        out_dir: &Path,
        trace_id: &str,
        preferred_mic: Option<&str>,
        keep_mic_stream_open: bool,
        capture_tuning: CaptureTuning,
    ) -> Result<PathBuf> {
        let (response_tx, response_rx) = bounded(1);
        self.command_tx
            .send(AudioCommand::Start {
                out_dir: out_dir.to_path_buf(),
                trace_id: trace_id.to_owned(),
                preferred_mic: preferred_mic.map(ToOwned::to_owned),
                keep_mic_stream_open,
                capture_tuning: capture_tuning.sanitize(),
                response: response_tx,
            })
            .context("failed sending start command to audio worker")?;

        let response = response_rx
            .recv()
            .context("audio worker disconnected while starting recording")?;
        response.map_err(|err| anyhow!(err))
    }

    pub fn stop_recording(&self) -> Result<(PathBuf, u64)> {
        let (response_tx, response_rx) = bounded(1);
        self.command_tx
            .send(AudioCommand::Stop {
                response: response_tx,
            })
            .context("failed sending stop command to audio worker")?;

        let response = response_rx
            .recv()
            .context("audio worker disconnected while stopping recording")?;
        response.map_err(|err| anyhow!(err))
    }

    pub fn input_level_percent(&self) -> u8 {
        let peak = self.input_level_peak.swap(0, Ordering::Relaxed);
        let percent = ((peak as f32 / i16::MAX as f32) * 100.0).round();
        percent.clamp(0.0, 100.0) as u8
    }
}

fn has_persistent_recording(persistent: &Option<PersistentCapture>) -> bool {
    persistent
        .as_ref()
        .and_then(|capture| capture.active_path.as_ref())
        .is_some()
}

fn ensure_persistent_capture(
    persistent: &mut Option<PersistentCapture>,
    preferred_mic: Option<&str>,
    capture_tuning: CaptureTuning,
    input_level_peak: Arc<AtomicU32>,
) -> Result<()> {
    let requested = preferred_mic.map(ToOwned::to_owned);
    let sanitized_tuning = capture_tuning.sanitize();
    let should_rebuild = match persistent.as_ref() {
        Some(existing) => {
            existing.preferred_mic != requested || existing.capture_tuning != sanitized_tuning
        }
        None => true,
    };

    if should_rebuild {
        if let Some(existing) = persistent.take() {
            shutdown_persistent_capture(existing)?;
        }
        *persistent = Some(start_persistent_capture(
            requested,
            sanitized_tuning,
            input_level_peak,
        )?);
    }

    Ok(())
}

fn start_persistent_capture(
    preferred_mic: Option<String>,
    capture_tuning: CaptureTuning,
    input_level_peak: Arc<AtomicU32>,
) -> Result<PersistentCapture> {
    let host = cpal::default_host();
    let device = pick_input_device(&host, preferred_mic.as_deref())?;
    let supported_config = device
        .default_input_config()
        .context("failed loading default input config")?;
    let config: StreamConfig = supported_config.clone().into();
    let sample_rate_hz = config.sample_rate.0;
    let target_rate = output_sample_rate(sample_rate_hz);
    let max_buffer_samples =
        (((target_rate as u64) * PERSISTENT_PREROLL_MS) / 1000).max(1) as usize;
    let processor = spawn_processor(
        sample_rate_hz,
        capture_tuning.audio_quality_preset,
        max_buffer_samples,
        None,
        false,
    )?;

    let stream_result = match supported_config.sample_format() {
        SampleFormat::F32 => build_stream_f32(
            &device,
            &config,
            processor.sample_tx(),
            processor_fatal_error(&processor),
            processor.overflow_count(),
            SampleProcessor::new(sample_rate_hz, capture_tuning),
            input_level_peak,
        ),
        SampleFormat::I16 => build_stream_i16(
            &device,
            &config,
            processor.sample_tx(),
            processor_fatal_error(&processor),
            processor.overflow_count(),
            SampleProcessor::new(sample_rate_hz, capture_tuning),
            input_level_peak,
        ),
        SampleFormat::U16 => build_stream_u16(
            &device,
            &config,
            processor.sample_tx(),
            processor_fatal_error(&processor),
            processor.overflow_count(),
            SampleProcessor::new(sample_rate_hz, capture_tuning),
            input_level_peak,
        ),
        other => Err(anyhow!("unsupported sample format: {other:?}")),
    };

    let stream = match stream_result {
        Ok(stream) => stream,
        Err(err) => {
            let _ = processor.shutdown();
            return Err(err);
        }
    };

    if let Err(err) = stream.play() {
        drop(stream);
        let _ = processor.shutdown();
        return Err(anyhow!(err)).context("failed starting persistent audio input stream");
    }

    Ok(PersistentCapture {
        preferred_mic,
        capture_tuning,
        sample_rate_hz,
        processor,
        _stream: stream,
        active_path: None,
    })
}

fn start_persistent_recording(
    persistent: Option<&mut PersistentCapture>,
    out_dir: &Path,
    trace_id: &str,
) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("failed creating recording dir {}", out_dir.display()))?;

    let capture = persistent.ok_or_else(|| anyhow!("persistent capture not initialized"))?;
    if capture.active_path.is_some() {
        return Err(anyhow!("recording already in progress"));
    }

    let out_path = out_dir.join(format!("{}.wav", trace_id));
    let writer = create_wav_writer(&out_path, output_sample_rate(capture.sample_rate_hz))?;
    capture.processor.start_writer(writer)?;
    capture.active_path = Some(out_path.clone());
    Ok(out_path)
}

fn stop_persistent_recording(persistent: Option<&mut PersistentCapture>) -> Result<(PathBuf, u64)> {
    let capture = persistent.ok_or_else(|| anyhow!("persistent capture not initialized"))?;
    let path = capture
        .active_path
        .take()
        .ok_or_else(|| anyhow!("stop_recording called without active recording"))?;

    let dropped = capture.processor.stop_writer()?;
    Ok((path, dropped))
}

fn shutdown_persistent_capture(capture: PersistentCapture) -> Result<()> {
    drop(capture._stream);
    capture.processor.shutdown()
}

fn start_recording_inner(
    out_dir: &Path,
    trace_id: &str,
    preferred_mic: Option<&str>,
    capture_tuning: CaptureTuning,
    input_level_peak: Arc<AtomicU32>,
) -> Result<ActiveRecording> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("failed creating recording dir {}", out_dir.display()))?;

    let host = cpal::default_host();
    let device = pick_input_device(&host, preferred_mic)?;
    let supported_config = device
        .default_input_config()
        .context("failed loading default input config")?;
    let config: StreamConfig = supported_config.clone().into();

    let out_path = out_dir.join(format!("{}.wav", trace_id));
    let source_rate = config.sample_rate.0;
    let writer = create_wav_writer(&out_path, output_sample_rate(source_rate))?;
    let processor = spawn_processor(
        source_rate,
        capture_tuning.audio_quality_preset,
        0,
        Some(writer),
        true,
    )?;

    let stream_result = match supported_config.sample_format() {
        SampleFormat::F32 => build_stream_f32(
            &device,
            &config,
            processor.sample_tx(),
            processor_fatal_error(&processor),
            processor.overflow_count(),
            SampleProcessor::new(config.sample_rate.0, capture_tuning),
            input_level_peak,
        ),
        SampleFormat::I16 => build_stream_i16(
            &device,
            &config,
            processor.sample_tx(),
            processor_fatal_error(&processor),
            processor.overflow_count(),
            SampleProcessor::new(config.sample_rate.0, capture_tuning),
            input_level_peak,
        ),
        SampleFormat::U16 => build_stream_u16(
            &device,
            &config,
            processor.sample_tx(),
            processor_fatal_error(&processor),
            processor.overflow_count(),
            SampleProcessor::new(config.sample_rate.0, capture_tuning),
            input_level_peak,
        ),
        other => Err(anyhow!("unsupported sample format: {other:?}")),
    };

    let stream = match stream_result {
        Ok(stream) => stream,
        Err(err) => {
            let _ = processor.shutdown();
            return Err(err);
        }
    };

    if let Err(err) = stream.play() {
        drop(stream);
        let _ = processor.shutdown();
        return Err(anyhow!(err)).context("failed starting audio input stream");
    }

    Ok(ActiveRecording {
        path: out_path,
        processor,
        stream,
    })
}

fn stop_recording_inner(recording: ActiveRecording) -> Result<(PathBuf, u64)> {
    let path = recording.path;
    let dropped = recording.processor.overflow_count.load(Ordering::Relaxed);
    drop(recording.stream);
    recording.processor.shutdown()?;
    Ok((path, dropped))
}

fn pick_input_device(host: &cpal::Host, preferred_name: Option<&str>) -> Result<cpal::Device> {
    if let Some(preferred_name) = preferred_name {
        for device in host
            .input_devices()
            .context("failed enumerating input devices")?
        {
            if device
                .name()
                .map(|name| name == preferred_name)
                .unwrap_or(false)
            {
                return Ok(device);
            }
        }
    }

    host.default_input_device()
        .ok_or_else(|| anyhow!("no default microphone found"))
}

fn create_wav_writer(
    out_path: &Path,
    sample_rate_hz: u32,
) -> Result<hound::WavWriter<BufWriter<File>>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    hound::WavWriter::create(out_path, spec)
        .with_context(|| format!("failed creating wav file {}", out_path.display()))
}

fn processor_fatal_error(handle: &ProcessorHandle) -> SharedError {
    handle.fatal_error.clone()
}

fn spawn_processor(
    source_rate_hz: u32,
    quality_preset: AudioQualityPreset,
    max_buffer_samples: usize,
    initial_writer: Option<hound::WavWriter<BufWriter<File>>>,
    flush_on_shutdown: bool,
) -> Result<ProcessorHandle> {
    let (sample_tx, sample_rx) = bounded::<Vec<i16>>(STAGING_CHANNEL_CAPACITY);
    let (command_tx, command_rx) = unbounded::<ProcessorCommand>();
    let fatal_error = Arc::new(Mutex::new(None));
    let overflow_count = Arc::new(AtomicU64::new(0));
    let thread_error = fatal_error.clone();
    let thread_overflow = overflow_count.clone();
    let mut state = ProcessorState::new(
        source_rate_hz,
        quality_preset,
        max_buffer_samples,
        initial_writer,
        flush_on_shutdown,
        thread_error,
        thread_overflow,
    )?;

    let join_handle = std::thread::spawn(move || {
        run_processor_loop(sample_rx, command_rx, &mut state);
    });

    Ok(ProcessorHandle {
        sample_tx,
        command_tx,
        fatal_error,
        overflow_count,
        join_handle: Some(join_handle),
    })
}

fn run_processor_loop(
    sample_rx: Receiver<Vec<i16>>,
    command_rx: Receiver<ProcessorCommand>,
    state: &mut ProcessorState,
) {
    loop {
        select! {
            recv(command_rx) -> message => {
                match message {
                    Ok(ProcessorCommand::StartWriter { writer, response }) => {
                        drain_pending_samples(&sample_rx, state);
                        let result = state.start_writer(writer).map_err(|err| err.to_string());
                        let _ = response.send(result);
                    }
                    Ok(ProcessorCommand::StopWriter { response }) => {
                        drain_pending_samples(&sample_rx, state);
                        let result = state.stop_writer().map_err(|err| err.to_string());
                        let _ = response.send(result);
                    }
                    Ok(ProcessorCommand::Shutdown { response }) => {
                        drain_pending_samples(&sample_rx, state);
                        let result = state.finish().map_err(|err| err.to_string());
                        let _ = response.send(result);
                        return;
                    }
                    Err(_) => {
                        drain_pending_samples(&sample_rx, state);
                        let _ = state.finish();
                        return;
                    }
                }
            }
            recv(sample_rx) -> samples => {
                match samples {
                    Ok(samples) => {
                        if let Err(err) = state.process_samples(&samples) {
                            set_shared_error(&state.fatal_error, err.to_string());
                        }
                    }
                    Err(_) => {
                        let _ = state.finish();
                        return;
                    }
                }
            }
        }
    }
}

fn drain_pending_samples(sample_rx: &Receiver<Vec<i16>>, state: &mut ProcessorState) {
    while let Ok(samples) = sample_rx.try_recv() {
        if let Err(err) = state.process_samples(&samples) {
            set_shared_error(&state.fatal_error, err.to_string());
            break;
        }
    }
}

fn update_input_level_peak(input_level_peak: &Arc<AtomicU32>, sample: i16) {
    let peak = (sample as i32).unsigned_abs();
    input_level_peak.fetch_max(peak, Ordering::Relaxed);
}

fn stage_callback_batch(
    sample_tx: &Sender<Vec<i16>>,
    batch: Vec<i16>,
    fatal_error: &SharedError,
    overflow_count: &AtomicU64,
) {
    if batch.is_empty() || shared_error_message(fatal_error).is_some() {
        return;
    }

    if let Err(err) = sample_tx.try_send(batch) {
        match err {
            TrySendError::Full(_) => {
                overflow_count.fetch_add(1, Ordering::Relaxed);
            }
            TrySendError::Disconnected(_) => {
                set_shared_error(fatal_error, "audio processing worker disconnected");
            }
        }
    }
}

fn build_stream_f32(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_tx: Sender<Vec<i16>>,
    fatal_error: SharedError,
    overflow_count: Arc<AtomicU64>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream_error = fatal_error.clone();
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            let mut batch = Vec::with_capacity(data.len() / channels.max(1));
            for frame in data.chunks(channels) {
                let sample = processor.process_f32_frame(frame);
                update_input_level_peak(&input_level_peak, sample);
                batch.push(sample);
            }
            stage_callback_batch(&sample_tx, batch, &fatal_error, &overflow_count);
        },
        move |err| {
            set_shared_error(&stream_error, format!("audio stream error: {err}"));
            eprintln!("audio stream error: {err}");
        },
        None,
    )?;

    Ok(stream)
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_tx: Sender<Vec<i16>>,
    fatal_error: SharedError,
    overflow_count: Arc<AtomicU64>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream_error = fatal_error.clone();
    let stream = device.build_input_stream(
        config,
        move |data: &[i16], _| {
            let mut batch = Vec::with_capacity(data.len() / channels.max(1));
            for frame in data.chunks(channels) {
                let sample = processor.process_i16_frame(frame);
                update_input_level_peak(&input_level_peak, sample);
                batch.push(sample);
            }
            stage_callback_batch(&sample_tx, batch, &fatal_error, &overflow_count);
        },
        move |err| {
            set_shared_error(&stream_error, format!("audio stream error: {err}"));
            eprintln!("audio stream error: {err}");
        },
        None,
    )?;

    Ok(stream)
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_tx: Sender<Vec<i16>>,
    fatal_error: SharedError,
    overflow_count: Arc<AtomicU64>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream_error = fatal_error.clone();
    let stream = device.build_input_stream(
        config,
        move |data: &[u16], _| {
            let mut batch = Vec::with_capacity(data.len() / channels.max(1));
            for frame in data.chunks(channels) {
                let sample = processor.process_u16_frame(frame);
                update_input_level_peak(&input_level_peak, sample);
                batch.push(sample);
            }
            stage_callback_batch(&sample_tx, batch, &fatal_error, &overflow_count);
        },
        move |err| {
            set_shared_error(&stream_error, format!("audio stream error: {err}"));
            eprintln!("audio stream error: {err}");
        },
        None,
    )?;

    Ok(stream)
}

fn set_shared_error(shared_error: &SharedError, message: impl Into<String>) {
    let mut shared = shared_error.lock();
    if shared.is_none() {
        *shared = Some(message.into());
    }
}

fn shared_error_message(shared_error: &SharedError) -> Option<String> {
    shared_error.lock().clone()
}

fn select_i16_frame(frame: &[i16], audio_channel_mode: AudioChannelMode) -> i16 {
    if frame.is_empty() {
        return 0;
    }

    match audio_channel_mode {
        AudioChannelMode::Left => frame[0],
        AudioChannelMode::Right => frame.get(1).copied().unwrap_or(frame[0]),
        AudioChannelMode::MonoMix => {
            let sum = frame.iter().map(|sample| *sample as i32).sum::<i32>();
            (sum / frame.len() as i32) as i16
        }
    }
}

fn select_u16_frame(frame: &[u16], audio_channel_mode: AudioChannelMode) -> i16 {
    if frame.is_empty() {
        return 0;
    }

    match audio_channel_mode {
        AudioChannelMode::Left => u16_to_i16(frame[0]),
        AudioChannelMode::Right => u16_to_i16(frame.get(1).copied().unwrap_or(frame[0])),
        AudioChannelMode::MonoMix => {
            let sum = frame
                .iter()
                .map(|sample| u16_to_i16(*sample) as i32)
                .sum::<i32>();
            (sum / frame.len() as i32) as i16
        }
    }
}

fn select_f32_frame(frame: &[f32], audio_channel_mode: AudioChannelMode) -> i16 {
    if frame.is_empty() {
        return 0;
    }

    match audio_channel_mode {
        AudioChannelMode::Left => f32_to_i16(frame[0]),
        AudioChannelMode::Right => f32_to_i16(frame.get(1).copied().unwrap_or(frame[0])),
        AudioChannelMode::MonoMix => {
            let sum = frame.iter().copied().sum::<f32>();
            f32_to_i16(sum / frame.len() as f32)
        }
    }
}

fn u16_to_i16(sample: u16) -> i16 {
    (sample as i32 - i16::MAX as i32 - 1) as i16
}

fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("buttervoice-{name}-{ts}.wav"))
    }

    fn generate_sine(rate: u32, frames: usize) -> Vec<i16> {
        (0..frames)
            .map(|idx| {
                let phase = 2.0 * PI * 440.0 * idx as f32 / rate as f32;
                f32_to_i16(phase.sin() * 0.7)
            })
            .collect()
    }

    #[test]
    fn speech_resampler_passthrough_for_whisper_rate_and_below() {
        let samples = vec![1, -2, 3, -4, 5];
        let mut resampler = SpeechResampler::new(
            WHISPER_SAMPLE_RATE,
            WHISPER_SAMPLE_RATE,
            AudioQualityPreset::Balanced,
        )
        .unwrap();
        let mut output = Vec::new();
        resampler.process_batch(&samples, &mut output).unwrap();
        resampler.flush(&mut output).unwrap();
        assert_eq!(output, samples);

        let mut resampler =
            SpeechResampler::new(8_000, WHISPER_SAMPLE_RATE, AudioQualityPreset::Balanced).unwrap();
        let mut output = Vec::new();
        resampler.process_batch(&samples, &mut output).unwrap();
        resampler.flush(&mut output).unwrap();
        assert_eq!(output, samples);
    }

    #[test]
    fn speech_resampler_downsamples_48k_to_16k() {
        let input = generate_sine(48_000, 4_800);
        let mut resampler =
            SpeechResampler::new(48_000, WHISPER_SAMPLE_RATE, AudioQualityPreset::Balanced)
                .unwrap();
        let mut output = Vec::new();
        resampler.process_batch(&input, &mut output).unwrap();
        resampler.flush(&mut output).unwrap();

        assert!(
            output.len() >= 1_568 && output.len() <= 1_800,
            "unexpected 48k->16k output length: {}",
            output.len()
        );
    }

    #[test]
    fn speech_resampler_downsamples_44k1_to_16k() {
        let input = generate_sine(44_100, 4_410);
        let mut resampler =
            SpeechResampler::new(44_100, WHISPER_SAMPLE_RATE, AudioQualityPreset::Balanced)
                .unwrap();
        let mut output = Vec::new();
        resampler.process_batch(&input, &mut output).unwrap();
        resampler.flush(&mut output).unwrap();

        assert!(
            output.len() >= 1_568 && output.len() <= 1_800,
            "unexpected 44.1k->16k output length: {}",
            output.len()
        );
    }

    #[test]
    fn speech_resampler_trims_initial_delay() {
        let input = vec![i16::MAX; 4_500];
        let mut resampler =
            SpeechResampler::new(48_000, WHISPER_SAMPLE_RATE, AudioQualityPreset::Balanced)
                .unwrap();
        let mut output = Vec::new();
        resampler.process_batch(&input, &mut output).unwrap();
        resampler.flush(&mut output).unwrap();

        let first_non_zero = output.iter().position(|sample| sample.unsigned_abs() > 16);
        assert!(
            matches!(first_non_zero, Some(idx) if idx < 128),
            "unexpected first non-zero index: {first_non_zero:?}"
        );
    }

    #[test]
    fn persistent_ring_buffer_caps_and_is_written_into_recording() {
        let fatal_error = Arc::new(Mutex::new(None));
        let overflow_count = Arc::new(AtomicU64::new(0));
        let writer_path = unique_temp_path("persistent-preroll");
        let writer = create_wav_writer(&writer_path, WHISPER_SAMPLE_RATE).unwrap();
        let mut state = ProcessorState::new(
            WHISPER_SAMPLE_RATE,
            AudioQualityPreset::Balanced,
            4,
            None,
            false,
            fatal_error,
            overflow_count,
        )
        .unwrap();

        state.process_samples(&[1, 2, 3, 4, 5, 6]).unwrap();
        assert_eq!(
            state
                .ring_buffer
                .as_ref()
                .map(|buffer| buffer.iter().copied().collect::<Vec<_>>())
                .unwrap(),
            vec![3, 4, 5, 6]
        );

        state.start_writer(writer).unwrap();
        state.stop_writer().unwrap();

        let reader = hound::WavReader::open(&writer_path).unwrap();
        let samples = reader
            .into_samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(samples, vec![3, 4, 5, 6]);

        let _ = std::fs::remove_file(writer_path);
    }

    #[test]
    fn audio_quality_profiles_match_expected_constants() {
        let balanced = AudioQualityPreset::Balanced.profile();
        assert_eq!(balanced.sinc_len, 256);
        assert_eq!(balanced.oversampling_factor, 256);

        let best = AudioQualityPreset::BestAccuracy.profile();
        assert_eq!(best.sinc_len, 512);
        assert_eq!(best.oversampling_factor, 256);

        let low = AudioQualityPreset::LowCpu.profile();
        assert_eq!(low.sinc_len, 128);
        assert_eq!(low.oversampling_factor, 128);
    }
}
