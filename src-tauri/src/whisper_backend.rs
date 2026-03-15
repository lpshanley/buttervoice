use std::fs::{self, File};
use std::io::{Read, Write};
#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use crate::app_state::ModelDownloadProgress;
use crate::models;
use crate::settings::ComputeMode;

const REQUIRED_SHARED_LIBS: &[&str] = &[
    "libwhisper.1.dylib",
    "libggml.0.dylib",
    "libggml-cpu.0.dylib",
    "libggml-blas.0.dylib",
    "libggml-metal.0.dylib",
    "libggml-base.0.dylib",
];

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

#[derive(Debug)]
pub struct WhisperBackend {
    whisper_bin: PathBuf,
    manifest_path: PathBuf,
    model_cache_dir: PathBuf,
    runtime_state: Mutex<RuntimeState>,
}

#[derive(Debug, Default)]
struct RuntimeState {
    effective_compute_mode: Option<String>,
    last_fallback_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BackendManifest {
    artifacts: Vec<BackendArtifact>,
}

#[derive(Debug, Deserialize)]
struct BackendArtifact {
    target: String,
    path: String,
    sha256: String,
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
    pub fn new(
        whisper_bin: PathBuf,
        manifest_path: PathBuf,
        model_cache_dir: PathBuf,
    ) -> Result<Self> {
        let backend = Self {
            whisper_bin,
            manifest_path,
            model_cache_dir,
            runtime_state: Mutex::new(RuntimeState::default()),
        };

        backend.ensure_paths()?;
        backend.prepare_runtime_bundle()?;
        backend.verify_binary_integrity()?;

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

        // Trim trailing silence to reduce Whisper hallucinations on dead air.
        if let Err(err) = trim_trailing_silence(&request.audio_path) {
            eprintln!(
                "warning: failed trimming trailing silence for {}: {err:#}",
                request.audio_path.display()
            );
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

        let execution_order = match request.compute_mode {
            ComputeMode::Auto => vec![EffectiveComputeMode::Gpu, EffectiveComputeMode::Cpu],
            ComputeMode::Cpu => vec![EffectiveComputeMode::Cpu],
            ComputeMode::Gpu => vec![EffectiveComputeMode::Gpu, EffectiveComputeMode::Cpu],
        };

        let mut run_errors = Vec::new();
        for (index, mode) in execution_order.iter().enumerate() {
            match self.run_whisper_cli(request, *mode, &model_path) {
                Ok((text, duration_ms)) => {
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
                        backend: Some(format!("whispercpp/{}", mode.as_str())),
                        effective_compute_mode: Some(mode.as_str().to_string()),
                        fallback_reason,
                        error_code: None,
                        error_message: None,
                    });
                }
                Err(err) => {
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
            &format!("whisper.cpp failed: {}", run_errors.join("; ")),
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
        Ok(())
    }

    pub fn warm_up(&self, model_id: &str, compute_mode: ComputeMode) -> Result<()> {
        let model_path = self.download_model(model_id)?;
        let runs_dir = self.model_cache_dir.join("runs");
        fs::create_dir_all(&runs_dir)
            .with_context(|| format!("failed creating run directory {}", runs_dir.display()))?;

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let request_id = format!("warmup-{ts}");
        let warmup_audio = runs_dir.join(format!("{request_id}.wav"));
        write_warmup_wav(&warmup_audio)?;

        let request = TranscribeRequest {
            request_id: request_id.clone(),
            audio_path: warmup_audio.clone(),
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
            match self.run_whisper_cli(&request, mode, &model_path) {
                Ok(_) => {
                    cleanup_warmup_run_artifacts(&runs_dir, &request_id, mode);
                    let _ = fs::remove_file(&warmup_audio);
                    return Ok(());
                }
                Err(err) => {
                    cleanup_warmup_run_artifacts(&runs_dir, &request_id, mode);
                    run_errors.push(format!("{}: {err}", mode.as_str()));
                }
            }
        }

        let _ = fs::remove_file(&warmup_audio);
        Err(anyhow!(
            "warm-up failed for model '{model_id}': {}",
            run_errors.join("; ")
        ))
    }

    pub fn backend_status(&self, selected_compute_mode: ComputeMode) -> BackendStatus {
        let state = self.runtime_state.lock();
        let binary_available = self.whisper_bin.exists();

        BackendStatus {
            ok: binary_available,
            backend: "whispercpp".to_string(),
            active_provider: "local_whispercpp".to_string(),
            provider_label: "local/whispercpp".to_string(),
            provider_ok: binary_available,
            provider_error: None,
            remote_base_url: None,
            remote_model: None,
            binary_available,
            binary_path: Some(self.whisper_bin.display().to_string()),
            selected_compute_mode: selected_compute_mode.as_str().to_string(),
            effective_compute_mode: state.effective_compute_mode.clone(),
            last_fallback_reason: state.last_fallback_reason.clone(),
        }
    }

    pub fn verify_binary_integrity(&self) -> Result<()> {
        if !self.whisper_bin.exists() {
            bail!(
                "bundled whisper-cli binary missing at {}",
                self.whisper_bin.display()
            );
        }

        #[cfg(target_family = "unix")]
        {
            let mode = fs::metadata(&self.whisper_bin)
                .with_context(|| format!("failed reading {}", self.whisper_bin.display()))?
                .permissions()
                .mode();
            if mode & 0o111 == 0 {
                bail!(
                    "bundled whisper-cli is not executable: {}",
                    self.whisper_bin.display()
                );
            }
        }

        let manifest = self.load_manifest()?;
        let target = current_backend_target()?;
        let artifact = manifest
            .artifacts
            .iter()
            .find(|item| item.target == target)
            .ok_or_else(|| anyhow!("manifest missing artifact for target {target}"))?;

        let binary_name = self
            .whisper_bin
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !artifact.path.ends_with(binary_name) {
            bail!(
                "manifest path '{}' does not match binary {}",
                artifact.path,
                self.whisper_bin.display()
            );
        }

        let actual_sha = sha256_file(&self.whisper_bin)?;
        if actual_sha != artifact.sha256.to_lowercase() {
            bail!(
                "bundled whisper-cli checksum mismatch for target {target}: expected {}, got {}",
                artifact.sha256,
                actual_sha
            );
        }

        self.verify_binary_architecture(&self.whisper_bin, target)?;
        self.verify_shared_libraries(target)?;

        Ok(())
    }

    fn ensure_paths(&self) -> Result<()> {
        let runs_dir = self.model_cache_dir.join("runs");
        fs::create_dir_all(&runs_dir)
            .with_context(|| format!("failed creating run directory {}", runs_dir.display()))?;
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

    fn run_whisper_cli(
        &self,
        request: &TranscribeRequest,
        mode: EffectiveComputeMode,
        model_path: &Path,
    ) -> Result<(String, u64)> {
        let runs_dir = self.model_cache_dir.join("runs");
        fs::create_dir_all(&runs_dir)
            .with_context(|| format!("failed creating run directory {}", runs_dir.display()))?;

        let output_prefix = runs_dir.join(format!(
            "run-{}-{}",
            sanitize_request_id(&request.request_id),
            mode.as_str()
        ));

        let mut command = Command::new(&self.whisper_bin);
        command
            .arg("-m")
            .arg(model_path)
            .arg("-f")
            .arg(&request.audio_path)
            .arg("-of")
            .arg(&output_prefix)
            .arg("-otxt")
            .arg("-np");

        if matches!(mode, EffectiveComputeMode::Cpu) {
            command.arg("-ng");
        }

        if request.threads > 0 {
            command.arg("--threads").arg(request.threads.to_string());
        }

        if !request.language.is_empty() && request.language != "auto" {
            command.arg("-l").arg(&request.language);
        }

        if request.beam_size > 1 {
            command
                .arg("--beam-size")
                .arg(request.beam_size.to_string());
        }

        if !request.prompt.is_empty() {
            command.arg("--prompt").arg(&request.prompt);
        }

        command
            .arg("--no-speech-thold")
            .arg(format!("{:.2}", request.no_speech_thold));

        command
            .arg("--temperature")
            .arg(format!("{:.2}", request.temperature));

        command
            .arg("--temperature-inc")
            .arg(format!("{:.2}", request.temperature_inc));

        let start = Instant::now();
        let output = command.output().with_context(|| {
            format!(
                "failed launching whisper-cli at {}",
                self.whisper_bin.display()
            )
        })?;
        let duration_ms = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            let mut detail = String::new();
            if !output.stderr.is_empty() {
                detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            }
            if detail.is_empty() && !output.stdout.is_empty() {
                detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
            if detail.len() > 800 {
                let start_idx = detail.len().saturating_sub(800);
                detail = detail[start_idx..].to_string();
            }
            bail!(
                "whisper.cpp exited with status {}: {}",
                output.status,
                if detail.is_empty() {
                    "no stderr output"
                } else {
                    &detail
                }
            );
        }

        let output_txt = output_prefix.with_extension("txt");
        let text = if output_txt.exists() {
            fs::read_to_string(&output_txt)
                .with_context(|| format!("failed reading output text {}", output_txt.display()))?
                .trim()
                .to_string()
        } else {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        Ok((text, duration_ms))
    }

    fn load_manifest(&self) -> Result<BackendManifest> {
        let raw = fs::read_to_string(&self.manifest_path).with_context(|| {
            format!(
                "failed reading backend manifest at {}",
                self.manifest_path.display()
            )
        })?;
        let manifest: BackendManifest = serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed parsing backend manifest at {}",
                self.manifest_path.display()
            )
        })?;
        Ok(manifest)
    }

    fn verify_binary_architecture(&self, binary_path: &Path, target: &str) -> Result<()> {
        let expected_arch_token = match target {
            "macos-aarch64" => "arm64",
            "macos-x86_64" => "x86_64",
            _ => return Ok(()),
        };

        let output = Command::new("file")
            .arg(binary_path)
            .output()
            .with_context(|| {
                format!(
                    "failed running `file` to validate architecture of {}",
                    binary_path.display()
                )
            })?;

        if !output.status.success() {
            bail!(
                "failed validating architecture of {}: {}",
                binary_path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let detail = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        if !detail.to_ascii_lowercase().contains(expected_arch_token) {
            bail!(
                "bundled whisper-cli architecture mismatch for {target}: expected {}, got {}",
                expected_arch_token,
                detail.trim()
            );
        }

        Ok(())
    }

    fn verify_shared_libraries(&self, target: &str) -> Result<()> {
        let binary_dir = self.whisper_bin.parent().ok_or_else(|| {
            anyhow!(
                "whisper binary path has no parent: {}",
                self.whisper_bin.display()
            )
        })?;
        let lib_dir = binary_dir.join("../lib");

        if !lib_dir.is_dir() {
            bail!(
                "bundled whisper library directory missing at {}",
                lib_dir.display()
            );
        }

        for lib_name in REQUIRED_SHARED_LIBS {
            let lib_path = lib_dir.join(lib_name);
            if !lib_path.is_file() {
                bail!(
                    "required bundled whisper library missing: {}",
                    lib_path.display()
                );
            }

            self.verify_binary_architecture(&lib_path, target)
                .with_context(|| {
                    format!("shared library architecture validation failed for {lib_name}")
                })?;
        }

        Ok(())
    }

    fn prepare_runtime_bundle(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let bundle_root = self
                .whisper_bin
                .parent()
                .and_then(|dir| dir.parent())
                .ok_or_else(|| {
                    anyhow!(
                        "whisper binary path has no bundle root: {}",
                        self.whisper_bin.display()
                    )
                })?;

            // Downloaded native helpers can retain quarantine bits and then trigger
            // Gatekeeper dialogs even though they are app-bundled resources.
            let status = Command::new("xattr")
                .arg("-dr")
                .arg("com.apple.quarantine")
                .arg(bundle_root)
                .status()
                .with_context(|| {
                    format!(
                        "failed removing quarantine attributes from {}",
                        bundle_root.display()
                    )
                })?;

            if !status.success() {
                bail!(
                    "failed removing quarantine attributes from {}",
                    bundle_root.display()
                );
            }
        }

        Ok(())
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
    }
}

fn current_backend_target() -> Result<&'static str> {
    if cfg!(target_arch = "aarch64") {
        return Ok("macos-aarch64");
    }
    if cfg!(target_arch = "x86_64") {
        return Ok("macos-x86_64");
    }
    bail!("unsupported architecture for bundled whisper backend")
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("failed opening binary for checksum: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed reading {} for checksum", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

fn sanitize_request_id(request_id: &str) -> String {
    let mut value: String = request_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect();
    if value.is_empty() {
        value = format!("{}", chrono_like_timestamp_ms());
    }
    value
}

fn chrono_like_timestamp_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn write_warmup_wav(path: &Path) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("failed creating warm-up audio file {}", path.display()))?;
    for idx in 0..(16_000 / 3) {
        let sample = if idx % 2 == 0 { 1200_i16 } else { -1200_i16 };
        writer
            .write_sample(sample)
            .context("failed writing warm-up waveform")?;
    }
    writer
        .finalize()
        .context("failed finalizing warm-up audio")?;
    Ok(())
}

fn cleanup_warmup_run_artifacts(runs_dir: &Path, request_id: &str, mode: EffectiveComputeMode) {
    let output_prefix = runs_dir.join(format!(
        "run-{}-{}",
        sanitize_request_id(request_id),
        mode.as_str()
    ));
    let _ = fs::remove_file(output_prefix.with_extension("txt"));
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

/// Trim trailing silence from a 16-bit WAV file in-place.
///
/// Walks backward from the end of the sample data to find the last sample
/// whose absolute value exceeds `SILENCE_THRESHOLD`, then rewrites the file
/// keeping only up to that point plus a small safety margin.  This prevents
/// Whisper from hallucinating on dead air at the end of a recording.
fn trim_trailing_silence(audio_file: &Path) -> Result<()> {
    const SILENCE_THRESHOLD: i16 = 256;
    const TAIL_MARGIN_MS: u32 = 150;

    let reader = hound::WavReader::open(audio_file)
        .with_context(|| format!("failed opening {} for silence trim", audio_file.display()))?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 {
        return Ok(());
    }

    let samples: Vec<i16> = reader
        .into_samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed reading samples for silence trim")?;

    if samples.is_empty() {
        return Ok(());
    }

    // Find last sample above threshold (walking backward is fast for typical
    // dictation recordings where silence is only at the tail).
    let last_voice_idx = samples
        .iter()
        .rposition(|s| s.unsigned_abs() > SILENCE_THRESHOLD as u16);

    let last_voice_idx = match last_voice_idx {
        Some(idx) => idx,
        None => return Ok(()), // entirely silent — leave as-is for the silent check
    };

    let margin_samples = ((spec.sample_rate * TAIL_MARGIN_MS) / 1000) as usize;
    let keep = (last_voice_idx + 1 + margin_samples).min(samples.len());

    // Only rewrite if we're actually trimming a meaningful amount (>250ms).
    let trimmed = samples.len() - keep;
    let min_trim_samples = (spec.sample_rate as usize) / 4;
    if trimmed < min_trim_samples {
        return Ok(());
    }

    let mut writer = hound::WavWriter::create(audio_file, spec)
        .with_context(|| format!("failed rewriting {} for silence trim", audio_file.display()))?;
    for &sample in &samples[..keep] {
        writer
            .write_sample(sample)
            .context("failed writing trimmed sample")?;
    }
    writer.finalize().context("failed finalizing trimmed wav")?;
    Ok(())
}
