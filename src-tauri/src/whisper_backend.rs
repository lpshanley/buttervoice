use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::app_state::ModelDownloadProgress;
use crate::models;
use crate::settings::ComputeMode;

/// Per-token confidence from Whisper inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfidence {
    /// Token text as emitted by whisper (may include leading space).
    pub text: String,
    /// Whisper probability for this token, 0.0..1.0.
    pub prob: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeRequest {
    pub request_id: String,
    pub audio_path: PathBuf,
    pub model_id: String,
    pub language: String,
    pub compute_mode: ComputeMode,
    pub beam_size: u32,
    pub prompt: String,
    pub no_speech_thold: f32,
    pub temperature: f32,
    pub temperature_inc: f32,
    pub threads: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResponse {
    pub request_id: String,
    pub text: String,
    pub duration_ms: u64,
    pub backend: Option<String>,
    pub effective_compute_mode: Option<String>,
    pub fallback_reason: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    /// Per-token confidence from local whisper inference.
    /// `None` for remote backends or error responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_confidences: Option<Vec<TokenConfidence>>,
    /// Highest per-segment no-speech probability from local whisper
    /// inference. High values indicate whisper believed the audio was
    /// silence even when it emitted text (hallucination signal).
    /// `None` for remote backends or error responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_speech_prob: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub ok: bool,
    pub backend: String,
    pub active_provider: String,
    pub provider_label: String,
    pub provider_ok: bool,
    pub provider_error: Option<String>,
    pub remote_base_url: Option<String>,
    pub remote_model: Option<String>,
    pub binary_available: bool,
    pub binary_path: Option<String>,
    pub selected_compute_mode: String,
    pub effective_compute_mode: Option<String>,
    pub last_fallback_reason: Option<String>,
}

pub struct WhisperBackend {
    model_cache_dir: PathBuf,
    runtime_state: Mutex<RuntimeState>,
    /// Cached whisper context: (model_id, context).
    /// Re-created when the model changes.
    cached_context: Mutex<Option<(String, Arc<WhisperContext>)>>,
}

// WhisperContext is Send+Sync but whisper-rs doesn't mark it as such in all
// versions.  The underlying C library is safe for concurrent read-only use
// after initialization, and we guard mutable access with a Mutex.
unsafe impl Send for WhisperBackend {}
unsafe impl Sync for WhisperBackend {}

impl std::fmt::Debug for WhisperBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhisperBackend")
            .field("model_cache_dir", &self.model_cache_dir)
            .finish()
    }
}

#[derive(Debug, Default)]
struct RuntimeState {
    effective_compute_mode: Option<String>,
    last_fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum EffectiveComputeMode {
    Cpu,
    Gpu,
}

impl EffectiveComputeMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

impl WhisperBackend {
    pub fn new(model_cache_dir: PathBuf) -> Result<Self> {
        let backend = Self {
            model_cache_dir,
            runtime_state: Mutex::new(RuntimeState::default()),
            cached_context: Mutex::new(None),
        };

        backend.ensure_paths()?;
        Ok(backend)
    }

    pub fn transcribe(&self, request: &TranscribeRequest) -> Result<TranscribeResponse> {
        if request.model_id.is_empty() {
            return Ok(json_error(
                "INVALID_REQUEST",
                "model_id is required",
                &request.request_id,
            ));
        }

        if !request.audio_path.exists() {
            return Ok(json_error(
                "AUDIO_DECODE_ERROR",
                &format!(
                    "audio file does not exist: {}",
                    request.audio_path.display()
                ),
                &request.request_id,
            ));
        }

        if is_probably_silent_wav(&request.audio_path) {
            return Ok(json_error(
                "AUDIO_SILENT",
                "recorded audio appears silent. Check ButterVoice microphone permission and selected input device.",
                &request.request_id,
            ));
        }

        let model_path = match self.download_model(&request.model_id) {
            Ok(path) => path,
            Err(err) => {
                return Ok(json_error(
                    "MODEL_DOWNLOAD_REQUIRED",
                    &format!("failed preparing model '{}': {err}", request.model_id),
                    &request.request_id,
                ))
            }
        };

        let audio_samples = match load_audio_samples(&request.audio_path) {
            Ok(samples) => samples,
            Err(err) => {
                return Ok(json_error(
                    "AUDIO_DECODE_ERROR",
                    &format!("failed loading audio: {err}"),
                    &request.request_id,
                ))
            }
        };

        let execution_order = match request.compute_mode {
            ComputeMode::Auto => vec![EffectiveComputeMode::Gpu, EffectiveComputeMode::Cpu],
            ComputeMode::Cpu => vec![EffectiveComputeMode::Cpu],
            ComputeMode::Gpu => vec![EffectiveComputeMode::Gpu, EffectiveComputeMode::Cpu],
        };

        let mut run_errors = Vec::new();
        for (index, mode) in execution_order.iter().enumerate() {
            match self.run_whisper(request, &audio_samples, *mode, &model_path) {
                Ok((text, duration_ms, token_confidences, no_speech_prob)) => {
                    let fallback_reason = if index == 0 {
                        None
                    } else {
                        Some(format!(
                            "{} failed; retried on cpu",
                            execution_order[0].as_str()
                        ))
                    };

                    {
                        let mut state = self.runtime_state.lock();
                        state.effective_compute_mode = Some(mode.as_str().to_string());
                        state.last_fallback_reason = fallback_reason.clone();
                    }

                    return Ok(TranscribeResponse {
                        request_id: request.request_id.clone(),
                        text,
                        duration_ms,
                        backend: Some(format!("whisper-rs/{}", mode.as_str())),
                        effective_compute_mode: Some(mode.as_str().to_string()),
                        fallback_reason,
                        error_code: None,
                        error_message: None,
                        token_confidences: Some(token_confidences),
                        no_speech_prob,
                    });
                }
                Err(err) => {
                    // Invalidate cached context on failure so next attempt
                    // re-creates it (possibly with different GPU settings).
                    *self.cached_context.lock() = None;
                    run_errors.push(format!("{}: {err}", mode.as_str()));
                }
            }
        }

        {
            let mut state = self.runtime_state.lock();
            state.last_fallback_reason = None;
        }

        Ok(json_error(
            "ENGINE_INIT_FAILED",
            &format!("whisper-rs failed: {}", run_errors.join("; ")),
            &request.request_id,
        ))
    }

    pub fn download_model(&self, model_id: &str) -> Result<PathBuf> {
        let (spec, destination) = self.model_destination(model_id)?;
        if destination.exists() && destination.metadata()?.len() > 0 {
            return Ok(destination);
        }

        let tmp_path = destination.with_extension("bin.tmp");
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .context("failed creating download client")?;

        let mut response = client
            .get(spec.download_url)
            .header(reqwest::header::USER_AGENT, "ButterVoice/1.0")
            .send()
            .with_context(|| format!("failed requesting model at {}", spec.download_url))?
            .error_for_status()
            .with_context(|| format!("download returned error status for {}", spec.id))?;

        let mut out_file = File::create(&tmp_path)
            .with_context(|| format!("failed creating temp model file {}", tmp_path.display()))?;
        response
            .copy_to(&mut out_file)
            .with_context(|| format!("failed downloading model {}", spec.id))?;
        out_file
            .sync_all()
            .with_context(|| format!("failed syncing model file {}", tmp_path.display()))?;

        fs::rename(&tmp_path, &destination).with_context(|| {
            format!(
                "failed moving model {} to {}",
                tmp_path.display(),
                destination.display()
            )
        })?;

        Ok(destination)
    }

    pub fn download_model_with_progress(
        &self,
        model_id: &str,
        app_handle: &AppHandle,
        cancel_flag: &Arc<AtomicBool>,
    ) -> Result<PathBuf> {
        let (spec, destination) = self.model_destination(model_id)?;
        if destination.exists() && destination.metadata()?.len() > 0 {
            return Ok(destination);
        }

        let tmp_path = destination.with_extension("bin.tmp");
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .context("failed creating download client")?;

        let response = client
            .get(spec.download_url)
            .header(reqwest::header::USER_AGENT, "ButterVoice/1.0")
            .send()
            .with_context(|| format!("failed requesting model at {}", spec.download_url))?
            .error_for_status()
            .with_context(|| format!("download returned error status for {}", spec.id))?;

        let total_bytes = response.content_length().unwrap_or(0);
        let mut reader = response;
        let mut out_file = File::create(&tmp_path)
            .with_context(|| format!("failed creating temp model file {}", tmp_path.display()))?;

        let mut downloaded_bytes: u64 = 0;
        let mut buf = [0u8; 32 * 1024];
        let mut last_emit = Instant::now();
        let model_id_owned = model_id.to_string();

        loop {
            if cancel_flag.load(Ordering::Relaxed) {
                drop(out_file);
                let _ = fs::remove_file(&tmp_path);
                bail!("download cancelled");
            }

            let n = reader
                .read(&mut buf)
                .with_context(|| format!("failed reading download stream for {}", spec.id))?;
            if n == 0 {
                break;
            }

            out_file
                .write_all(&buf[..n])
                .with_context(|| format!("failed writing model file {}", tmp_path.display()))?;
            downloaded_bytes += n as u64;

            if last_emit.elapsed().as_millis() >= 100 {
                let _ = app_handle.emit(
                    "model-download-progress",
                    ModelDownloadProgress {
                        model_id: model_id_owned.clone(),
                        downloaded_bytes,
                        total_bytes,
                        status: "downloading".to_string(),
                        error: None,
                    },
                );
                last_emit = Instant::now();
            }
        }

        out_file
            .sync_all()
            .with_context(|| format!("failed syncing model file {}", tmp_path.display()))?;

        fs::rename(&tmp_path, &destination).with_context(|| {
            format!(
                "failed moving model {} to {}",
                tmp_path.display(),
                destination.display()
            )
        })?;

        Ok(destination)
    }

    #[allow(dead_code)]
    pub fn list_downloaded_models(&self) -> Result<Vec<String>> {
        let mut downloaded = Vec::new();
        for model in models::available_models() {
            let (_, destination) = self.model_destination(&model.id)?;
            if destination.exists() && destination.metadata()?.len() > 0 {
                downloaded.push(model.id);
            }
        }
        downloaded.sort();
        Ok(downloaded)
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        let (_, destination) = self.model_destination(model_id)?;
        if destination.exists() {
            fs::remove_file(&destination)
                .with_context(|| format!("failed deleting model file {}", destination.display()))?;
        }
        // Invalidate cached context if we just deleted the loaded model.
        let mut cached = self.cached_context.lock();
        if cached.as_ref().is_some_and(|(id, _)| id == model_id) {
            *cached = None;
        }
        Ok(())
    }

    pub fn warm_up(&self, model_id: &str, compute_mode: ComputeMode) -> Result<()> {
        let model_path = self.download_model(model_id)?;

        // Generate a short warmup audio buffer (1/3 second of simple tone at 16kHz)
        let warmup_samples: Vec<f32> = (0..(16_000 / 3))
            .map(|idx| if idx % 2 == 0 { 0.037 } else { -0.037 })
            .collect();

        let request = TranscribeRequest {
            request_id: format!(
                "warmup-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            ),
            audio_path: PathBuf::new(), // Not used — we pass samples directly
            model_id: model_id.to_string(),
            language: "en".to_string(),
            compute_mode,
            beam_size: 1,
            prompt: String::new(),
            no_speech_thold: 0.6,
            temperature: 0.0,
            temperature_inc: 0.2,
            threads: 0,
        };

        let execution_order = match compute_mode {
            ComputeMode::Auto => vec![EffectiveComputeMode::Gpu, EffectiveComputeMode::Cpu],
            ComputeMode::Cpu => vec![EffectiveComputeMode::Cpu],
            ComputeMode::Gpu => vec![EffectiveComputeMode::Gpu, EffectiveComputeMode::Cpu],
        };

        let mut run_errors = Vec::new();
        for mode in execution_order {
            match self.run_whisper(&request, &warmup_samples, mode, &model_path) {
                Ok(_) => return Ok(()),
                Err(err) => {
                    *self.cached_context.lock() = None;
                    run_errors.push(format!("{}: {err}", mode.as_str()));
                }
            }
        }

        Err(anyhow!(
            "warm-up failed for model '{model_id}': {}",
            run_errors.join("; ")
        ))
    }

    pub fn backend_status(&self, selected_compute_mode: ComputeMode) -> BackendStatus {
        let state = self.runtime_state.lock();

        BackendStatus {
            ok: true,
            backend: "whisper-rs".to_string(),
            active_provider: "local_whispercpp".to_string(),
            provider_label: "local/whisper-rs".to_string(),
            provider_ok: true,
            provider_error: None,
            remote_base_url: None,
            remote_model: None,
            binary_available: true,
            binary_path: None,
            selected_compute_mode: selected_compute_mode.as_str().to_string(),
            effective_compute_mode: state.effective_compute_mode.clone(),
            last_fallback_reason: state.last_fallback_reason.clone(),
        }
    }

    fn ensure_paths(&self) -> Result<()> {
        fs::create_dir_all(&self.model_cache_dir).with_context(|| {
            format!(
                "failed creating model cache directory {}",
                self.model_cache_dir.display()
            )
        })?;
        Ok(())
    }

    fn model_destination(&self, model_id: &str) -> Result<(models::ModelDownloadSpec, PathBuf)> {
        let spec = models::download_spec(model_id)
            .ok_or_else(|| anyhow!("unsupported model_id '{model_id}'"))?;
        let model_dir = self.model_cache_dir.join("whisper.cpp");
        fs::create_dir_all(&model_dir)
            .with_context(|| format!("failed creating model directory {}", model_dir.display()))?;
        let destination = model_dir.join(spec.file_name);
        Ok((spec, destination))
    }

    fn get_or_create_context(
        &self,
        model_id: &str,
        model_path: &Path,
        use_gpu: bool,
    ) -> Result<Arc<WhisperContext>> {
        let mut cached = self.cached_context.lock();
        if let Some((id, ctx)) = cached.as_ref() {
            if id == model_id {
                return Ok(ctx.clone());
            }
        }

        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(use_gpu);

        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| anyhow!("model path is not valid UTF-8"))?,
            ctx_params,
        )
        .map_err(|e| anyhow!("failed to initialize whisper context: {e}"))?;

        let ctx = Arc::new(ctx);
        *cached = Some((model_id.to_string(), ctx.clone()));
        Ok(ctx)
    }

    fn run_whisper(
        &self,
        request: &TranscribeRequest,
        audio_samples: &[f32],
        mode: EffectiveComputeMode,
        model_path: &Path,
    ) -> Result<(String, u64, Vec<TokenConfidence>, Option<f32>)> {
        let use_gpu = matches!(mode, EffectiveComputeMode::Gpu);
        let ctx = self.get_or_create_context(&request.model_id, model_path, use_gpu)?;

        let mut params = if request.beam_size > 1 {
            FullParams::new(SamplingStrategy::BeamSearch {
                beam_size: request.beam_size as i32,
                patience: -1.0,
            })
        } else {
            FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
        };

        if !request.language.is_empty() && request.language != "auto" {
            params.set_language(Some(&request.language));
        }

        if !request.prompt.is_empty() {
            params.set_initial_prompt(&request.prompt);
        }

        params.set_no_speech_thold(request.no_speech_thold);
        params.set_temperature(request.temperature);
        params.set_temperature_inc(request.temperature_inc);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Suppress bracketed annotation tokens ([Music], (applause), ♪) at
        // sampling time — a common hallucination on silent/noisy audio.
        params.set_suppress_nst(true);

        if request.threads > 0 {
            params.set_n_threads(request.threads as i32);
        }

        let start = Instant::now();

        let mut state = ctx
            .create_state()
            .map_err(|e| anyhow!("failed to create whisper state: {e}"))?;

        state
            .full(params, audio_samples)
            .map_err(|e| anyhow!("whisper inference failed: {e}"))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        let mut text = String::new();
        let mut token_confidences = Vec::new();
        let mut max_no_speech_prob: Option<f32> = None;
        for segment in state.as_iter() {
            let segment_text = segment
                .to_str_lossy()
                .map_err(|e| anyhow!("failed to get segment text: {e}"))?;
            text.push_str(&segment_text);

            // Whisper's belief that the segment is silence; emitted text on
            // a high no-speech segment is a hallucination signal.
            let no_speech = segment.no_speech_probability();
            max_no_speech_prob = Some(max_no_speech_prob.map_or(no_speech, |m| m.max(no_speech)));

            for t in 0..segment.n_tokens() {
                let Some(token) = segment.get_token(t) else {
                    continue;
                };
                let tok_text = token
                    .to_str_lossy()
                    .map_err(|e| anyhow!("failed to get token {t} text: {e}"))?
                    .into_owned();
                let prob = token.token_probability();
                // Skip special tokens (empty, whitespace-only, [_BEG_], etc.)
                if !tok_text.trim().is_empty() && !tok_text.starts_with('[') {
                    token_confidences.push(TokenConfidence {
                        text: tok_text,
                        prob,
                    });
                }
            }
        }

        Ok((
            text.trim().to_string(),
            duration_ms,
            token_confidences,
            max_no_speech_prob,
        ))
    }
}

fn json_error(error_code: &str, error_message: &str, request_id: &str) -> TranscribeResponse {
    TranscribeResponse {
        request_id: request_id.to_string(),
        text: String::new(),
        duration_ms: 0,
        backend: None,
        effective_compute_mode: None,
        fallback_reason: None,
        error_code: Some(error_code.to_string()),
        error_message: Some(error_message.to_string()),
        token_confidences: None,
        no_speech_prob: None,
    }
}

/// Load audio samples from a WAV file as f32 PCM normalized to [-1.0, 1.0].
///
/// Whisper expects mono 16kHz f32 audio. The audio pipeline already resamples
/// to 16kHz mono before writing the WAV, so we just need to convert sample
/// format here.
fn load_audio_samples(audio_path: &Path) -> Result<Vec<f32>> {
    let reader = hound::WavReader::open(audio_path)
        .with_context(|| format!("failed opening audio file {}", audio_path.display()))?;
    let spec = reader.spec();

    if spec.bits_per_sample == 16 && spec.sample_format == hound::SampleFormat::Int {
        let samples: Vec<f32> = reader
            .into_samples::<i16>()
            .map(|s| s.map(|s| s as f32 / 32768.0))
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed reading i16 samples")?;
        Ok(samples)
    } else if spec.bits_per_sample == 32 && spec.sample_format == hound::SampleFormat::Float {
        let samples: Vec<f32> = reader
            .into_samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed reading f32 samples")?;
        Ok(samples)
    } else {
        bail!(
            "unsupported WAV format: {} bit {:?}",
            spec.bits_per_sample,
            spec.sample_format
        )
    }
}

fn is_probably_silent_wav(audio_file: &Path) -> bool {
    if !audio_file
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
    {
        return false;
    }

    let reader = match hound::WavReader::open(audio_file) {
        Ok(reader) => reader,
        Err(_) => return false,
    };

    let spec = reader.spec();
    if spec.bits_per_sample != 16 {
        return false;
    }

    let mut saw_sample = false;
    let mut peak = 0_i32;
    for sample in reader.into_samples::<i16>() {
        let sample = match sample {
            Ok(sample) => sample,
            Err(_) => return false,
        };
        saw_sample = true;
        let sample_abs = (sample as i32).abs();
        if sample_abs > peak {
            peak = sample_abs;
        }
        if peak >= 256 {
            return false;
        }
    }

    !saw_sample || peak < 256
}
