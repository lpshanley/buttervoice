use std::collections::VecDeque;
use std::f32::consts::PI;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, unbounded, Sender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::settings::{AudioChannelMode, HighPassFilter};

const PERSISTENT_PREROLL_MS: u64 = 600;
const MAX_INPUT_GAIN_DB: f32 = 24.0;
const WHISPER_SAMPLE_RATE: u32 = 16_000;

fn output_sample_rate(source_rate: u32) -> u32 {
    if source_rate > WHISPER_SAMPLE_RATE {
        WHISPER_SAMPLE_RATE
    } else {
        source_rate
    }
}

// ── Anti-aliasing low-pass filter (cascaded first-order IIR) ──

#[derive(Debug, Clone)]
struct AntiAliasLowPass {
    alpha: f32,
    y1: f32,
    y2: f32,
}

impl AntiAliasLowPass {
    fn new(source_rate: u32, target_rate: u32) -> Self {
        // Cutoff at 45% of target rate (= 90% of target Nyquist) to leave
        // transition-band headroom before the Nyquist limit.
        let cutoff_hz = target_rate as f32 * 0.45;
        let dt = 1.0 / source_rate as f32;
        let rc = 1.0 / (2.0 * PI * cutoff_hz);
        let alpha = dt / (rc + dt);
        Self {
            alpha,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, sample: f32) -> f32 {
        // Two cascaded first-order stages for ~-12 dB/octave rolloff.
        self.y1 += self.alpha * (sample - self.y1);
        self.y2 += self.alpha * (self.y1 - self.y2);
        self.y2
    }
}

// ── Arbitrary-ratio resampler with integrated anti-alias filter ──

#[derive(Debug, Clone)]
struct Resampler {
    active: bool,
    filter: AntiAliasLowPass,
    /// How much output-phase we accumulate per input sample (target / source).
    increment: f64,
    accumulator: f64,
}

impl Resampler {
    fn new(source_rate: u32, target_rate: u32) -> Self {
        let active = source_rate > target_rate;
        Self {
            active,
            filter: AntiAliasLowPass::new(source_rate, target_rate),
            increment: target_rate as f64 / source_rate as f64,
            accumulator: 0.0,
        }
    }

    /// Feed one input sample. Returns `Some(output)` when a decimated sample
    /// is ready, or `None` when accumulating.  For pass-through (no
    /// resampling) every input produces an output.
    fn push(&mut self, sample: i16) -> Option<i16> {
        if !self.active {
            return Some(sample);
        }

        let filtered = self.filter.process(sample as f32);
        self.accumulator += self.increment;

        if self.accumulator >= 1.0 {
            self.accumulator -= 1.0;
            Some(filtered.clamp(i16::MIN as f32, i16::MAX as f32).round() as i16)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicDevice {
    pub id: String,
    pub name: String,
}

struct RecordingSink {
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    resampler: Resampler,
    write_error: bool,
}

struct ActiveRecording {
    path: PathBuf,
    sink: Arc<Mutex<RecordingSink>>,
    stream: cpal::Stream,
}

struct PersistentCapture {
    preferred_mic: Option<String>,
    capture_tuning: CaptureTuning,
    sample_rate_hz: u32,
    callback_state: Arc<Mutex<PersistentCallbackState>>,
    _stream: cpal::Stream,
    active_path: Option<PathBuf>,
}

struct PersistentCallbackState {
    ring_buffer: VecDeque<i16>,
    max_buffer_samples: usize,
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    resampler: Resampler,
    write_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureTuning {
    pub audio_channel_mode: AudioChannelMode,
    pub input_gain_db: f32,
    pub high_pass_filter: HighPassFilter,
}

impl CaptureTuning {
    fn sanitize(self) -> Self {
        Self {
            audio_channel_mode: self.audio_channel_mode,
            input_gain_db: self
                .input_gain_db
                .clamp(-MAX_INPUT_GAIN_DB, MAX_INPUT_GAIN_DB),
            high_pass_filter: self.high_pass_filter,
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

enum AudioCommand {
    Start {
        out_dir: PathBuf,
        trace_id: String,
        preferred_mic: Option<String>,
        keep_mic_stream_open: bool,
        capture_tuning: CaptureTuning,
        response: Sender<std::result::Result<PathBuf, String>>,
    },
    Stop {
        response: Sender<std::result::Result<PathBuf, String>>,
    },
    Configure {
        preferred_mic: Option<String>,
        keep_mic_stream_open: bool,
        capture_tuning: CaptureTuning,
        response: Sender<std::result::Result<(), String>>,
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

                        persistent = None;

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
                            persistent = None;
                            input_level_peak_worker.store(0, Ordering::Relaxed);
                            Ok(())
                        };

                        let _ = response.send(result.map_err(|err| err.to_string()));
                    }
                }
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

    pub fn stop_recording(&self) -> Result<PathBuf> {
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
    let callback_state = Arc::new(Mutex::new(PersistentCallbackState {
        ring_buffer: VecDeque::with_capacity(max_buffer_samples + 1),
        max_buffer_samples,
        writer: None,
        resampler: Resampler::new(sample_rate_hz, WHISPER_SAMPLE_RATE),
        write_error: false,
    }));

    let stream = match supported_config.sample_format() {
        SampleFormat::F32 => build_persistent_stream_f32(
            &device,
            &config,
            callback_state.clone(),
            SampleProcessor::new(sample_rate_hz, capture_tuning),
            input_level_peak,
        )?,
        SampleFormat::I16 => build_persistent_stream_i16(
            &device,
            &config,
            callback_state.clone(),
            SampleProcessor::new(sample_rate_hz, capture_tuning),
            input_level_peak,
        )?,
        SampleFormat::U16 => build_persistent_stream_u16(
            &device,
            &config,
            callback_state.clone(),
            SampleProcessor::new(sample_rate_hz, capture_tuning),
            input_level_peak,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    stream
        .play()
        .context("failed starting persistent audio input stream")?;

    Ok(PersistentCapture {
        preferred_mic,
        capture_tuning,
        sample_rate_hz,
        callback_state,
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
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: output_sample_rate(capture.sample_rate_hz),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&out_path, spec)
        .with_context(|| format!("failed creating wav file {}", out_path.display()))?;

    {
        let mut state = capture.callback_state.lock();
        for sample in &state.ring_buffer {
            writer
                .write_sample(*sample)
                .context("failed writing preroll audio")?;
        }
        state.writer = Some(writer);
    }

    capture.active_path = Some(out_path.clone());
    Ok(out_path)
}

fn stop_persistent_recording(persistent: Option<&mut PersistentCapture>) -> Result<PathBuf> {
    let capture = persistent.ok_or_else(|| anyhow!("persistent capture not initialized"))?;
    let path = capture
        .active_path
        .take()
        .ok_or_else(|| anyhow!("stop_recording called without active recording"))?;

    let mut state = capture.callback_state.lock();
    let had_write_error = state.write_error;
    state.write_error = false;
    let writer = state
        .writer
        .take()
        .ok_or_else(|| anyhow!("persistent writer was not initialized for the active recording"))?;
    drop(state);

    writer
        .finalize()
        .context("failed finalizing persistent wav file")?;

    if had_write_error {
        eprintln!(
            "warning: one or more audio samples failed to write to {}",
            path.display()
        );
    }

    Ok(path)
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
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: output_sample_rate(source_rate),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let sink = Arc::new(Mutex::new(RecordingSink {
        writer: Some(
            hound::WavWriter::create(&out_path, spec)
                .with_context(|| format!("failed creating wav file {}", out_path.display()))?,
        ),
        resampler: Resampler::new(source_rate, WHISPER_SAMPLE_RATE),
        write_error: false,
    }));

    let stream = match supported_config.sample_format() {
        SampleFormat::F32 => build_stream_f32(
            &device,
            &config,
            sink.clone(),
            SampleProcessor::new(config.sample_rate.0, capture_tuning),
            input_level_peak,
        )?,
        SampleFormat::I16 => build_stream_i16(
            &device,
            &config,
            sink.clone(),
            SampleProcessor::new(config.sample_rate.0, capture_tuning),
            input_level_peak,
        )?,
        SampleFormat::U16 => build_stream_u16(
            &device,
            &config,
            sink.clone(),
            SampleProcessor::new(config.sample_rate.0, capture_tuning),
            input_level_peak,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    stream
        .play()
        .context("failed starting audio input stream")?;

    Ok(ActiveRecording {
        path: out_path,
        sink,
        stream,
    })
}

fn stop_recording_inner(recording: ActiveRecording) -> Result<PathBuf> {
    let path = recording.path;
    drop(recording.stream);

    let mut state = recording.sink.lock();
    let had_write_error = state.write_error;
    if let Some(writer) = state.writer.take() {
        writer.finalize().context("failed finalizing wav file")?;
    }
    drop(state);

    if had_write_error {
        eprintln!(
            "warning: one or more audio samples failed to write to {}",
            path.display()
        );
    }

    Ok(path)
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

fn update_input_level_peak(input_level_peak: &Arc<AtomicU32>, sample: i16) {
    let peak = (sample as i32).unsigned_abs();
    input_level_peak.fetch_max(peak, Ordering::Relaxed);
}

fn write_persistent_sample(
    state: &mut PersistentCallbackState,
    sample: i16,
    input_level_peak: &Arc<AtomicU32>,
) {
    update_input_level_peak(input_level_peak, sample);

    if let Some(out) = state.resampler.push(sample) {
        state.ring_buffer.push_back(out);
        if state.ring_buffer.len() > state.max_buffer_samples {
            let _ = state.ring_buffer.pop_front();
        }
        if let Some(writer) = state.writer.as_mut() {
            if writer.write_sample(out).is_err() {
                state.write_error = true;
            }
        }
    }
}

fn build_persistent_stream_f32(
    device: &cpal::Device,
    config: &StreamConfig,
    callback_state: Arc<Mutex<PersistentCallbackState>>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            let mut state = callback_state.lock();
            for frame in data.chunks(channels) {
                let sample = processor.process_f32_frame(frame);
                write_persistent_sample(&mut state, sample, &input_level_peak);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;

    Ok(stream)
}

fn build_persistent_stream_i16(
    device: &cpal::Device,
    config: &StreamConfig,
    callback_state: Arc<Mutex<PersistentCallbackState>>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[i16], _| {
            let mut state = callback_state.lock();
            for frame in data.chunks(channels) {
                let sample = processor.process_i16_frame(frame);
                write_persistent_sample(&mut state, sample, &input_level_peak);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;

    Ok(stream)
}

fn build_persistent_stream_u16(
    device: &cpal::Device,
    config: &StreamConfig,
    callback_state: Arc<Mutex<PersistentCallbackState>>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[u16], _| {
            let mut state = callback_state.lock();
            for frame in data.chunks(channels) {
                let sample = processor.process_u16_frame(frame);
                write_persistent_sample(&mut state, sample, &input_level_peak);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;

    Ok(stream)
}

fn write_recorded_sample(
    sink: &Arc<Mutex<RecordingSink>>,
    sample: i16,
    input_level_peak: &Arc<AtomicU32>,
) {
    update_input_level_peak(input_level_peak, sample);
    let mut state = sink.lock();

    if let Some(out) = state.resampler.push(sample) {
        if let Some(writer) = state.writer.as_mut() {
            if writer.write_sample(out).is_err() {
                state.write_error = true;
            }
        }
    }
}

fn build_stream_f32(
    device: &cpal::Device,
    config: &StreamConfig,
    sink: Arc<Mutex<RecordingSink>>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            for frame in data.chunks(channels) {
                let sample = processor.process_f32_frame(frame);
                write_recorded_sample(&sink, sample, &input_level_peak);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;

    Ok(stream)
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &StreamConfig,
    sink: Arc<Mutex<RecordingSink>>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[i16], _| {
            for frame in data.chunks(channels) {
                let sample = processor.process_i16_frame(frame);
                write_recorded_sample(&sink, sample, &input_level_peak);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;

    Ok(stream)
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &StreamConfig,
    sink: Arc<Mutex<RecordingSink>>,
    mut processor: SampleProcessor,
    input_level_peak: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[u16], _| {
            for frame in data.chunks(channels) {
                let sample = processor.process_u16_frame(frame);
                write_recorded_sample(&sink, sample, &input_level_peak);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;

    Ok(stream)
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
