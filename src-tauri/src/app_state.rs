use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{self, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use crossbeam_channel::bounded;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
use uuid::Uuid;

use crate::audio::{AudioCapture, CaptureTuning};
use crate::content_classification::{self, ContentClassificationResult};
use crate::hotkey_macos::HotkeyConfig;
use crate::llm_cleanup;
use crate::llm_guard;
use crate::permissions_macos;
use crate::persona;
use crate::post_process::PostProcessor;
use crate::secrets;
use crate::settings::{self, ComputeMode, OutputDestination, Settings, SettingsStore};
use crate::speech_backend::SpeechService;
use crate::text_inject_macos;
use crate::usage_stats::UsageStatsStore;
use crate::whisper_backend::{BackendStatus, TranscribeRequest};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationState {
    Idle,
    Recording,
    Transcribing,
    PostProcessing,
    Injecting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub status: String, // "downloading" | "completed" | "cancelled" | "error"
    pub error: Option<String>,
}

pub struct AppState {
    app_handle: AppHandle,
    instance_id: String,
    settings_store: SettingsStore,
    audio: AudioCapture,
    backend: SpeechService,
    recordings_dir: PathBuf,
    dictation_state: Mutex<DictationState>,
    recording_started_at: Mutex<Option<Instant>>,
    active_trace_id: Mutex<Option<String>>,
    transcript_logs: Mutex<Vec<TranscriptLogEntry>>,
    debug_logs: Mutex<Vec<DebugLogEntry>>,
    warmup_serial: Mutex<()>,
    output_destination: Arc<atomic::AtomicU8>,
    active_downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    hotkey_config: &'static HotkeyConfig,
    /// Tracks the last character of the most recently injected text so that
    /// consecutive dictations can be separated with a space when needed.
    last_injected_trailing_char: Mutex<Option<char>>,
    /// Tracks when text was last injected so stale context does not force
    /// a leading space on a fresh dictation.
    last_injected_at_ms: Mutex<Option<u64>>,
    post_processor: Mutex<PostProcessor>,
    llm_guard: llm_guard::LlmGuard,
    pipeline_metrics: Mutex<PipelineMetrics>,
    pub usage_stats: UsageStatsStore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptLogEntry {
    pub instance_id: String,
    pub trace_id: String,
    pub request_id: String,
    pub timestamp_ms: u64,
    pub model_id: String,
    pub backend: Option<String>,
    pub duration_ms: u64,
    pub recording_duration_ms: u64,
    pub transcription_duration_ms: u64,
    pub cleanup_roundtrip_duration_ms: u64,
    pub total_waterfall_duration_ms: u64,
    pub text: String,
    pub raw_text: Option<String>,
    pub post_process_text: Option<String>,
    pub is_final: bool,
    pub cleanup_requested: bool,
    pub cleanup_applied: bool,
    pub post_process_duration_ms: u64,
    pub post_process_edits_applied: u32,
    pub post_process_edits_rejected: u32,
    pub recording_file: Option<String>,
    pub classification_result: Option<ContentClassificationResult>,
    pub classification_duration_ms: u64,
    pub persona_id: Option<String>,
    pub persona_text: Option<String>,
    pub persona_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugLogEntry {
    pub instance_id: String,
    pub trace_id: Option<String>,
    pub timestamp_ms: u64,
    pub scope: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageLatencyHistogram {
    pub le_50_ms: u64,
    pub le_100_ms: u64,
    pub le_250_ms: u64,
    pub le_500_ms: u64,
    pub le_1000_ms: u64,
    pub gt_1000_ms: u64,
}

impl StageLatencyHistogram {
    fn observe(&mut self, duration_ms: u64) {
        if duration_ms <= 50 {
            self.le_50_ms = self.le_50_ms.saturating_add(1);
        } else if duration_ms <= 100 {
            self.le_100_ms = self.le_100_ms.saturating_add(1);
        } else if duration_ms <= 250 {
            self.le_250_ms = self.le_250_ms.saturating_add(1);
        } else if duration_ms <= 500 {
            self.le_500_ms = self.le_500_ms.saturating_add(1);
        } else if duration_ms <= 1000 {
            self.le_1000_ms = self.le_1000_ms.saturating_add(1);
        } else {
            self.gt_1000_ms = self.gt_1000_ms.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineFailureEvent {
    pub timestamp_ms: u64,
    pub stage: String,
    pub error_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineMetricsSnapshot {
    pub dictations_started: u64,
    pub dictations_succeeded: u64,
    pub dictations_failed: u64,
    pub pp_runs: u64,
    pub pp_edits_applied_total: u64,
    pub pp_edits_rejected_total: u64,
    pub llm_attempts: u64,
    pub llm_success: u64,
    pub llm_fail: u64,
    pub llm_timeout: u64,
    pub llm_skipped_circuit_open: u64,
    pub stage_latency_histograms: HashMap<String, StageLatencyHistogram>,
    pub last_100_failures: Vec<PipelineFailureEvent>,
    pub llm_circuit_open: bool,
    pub llm_circuit_open_until_ms: Option<u64>,
}

#[derive(Debug, Default)]
struct PipelineMetrics {
    snapshot: PipelineMetricsSnapshot,
    failures: VecDeque<PipelineFailureEvent>,
}

#[derive(Default)]
struct PostProcessStageOutcome {
    output: Option<String>,
    duration_ms: u64,
    edits_applied: u32,
    edits_rejected: u32,
    text_for_log: Option<String>,
}

#[derive(Default)]
struct LlmCleanupStageOutcome {
    output: Option<String>,
    roundtrip_duration_ms: u64,
}

#[derive(Default)]
struct ClassificationStageOutcome {
    result: Option<ContentClassificationResult>,
    duration_ms: u64,
}

#[derive(Default)]
struct PersonaStageOutcome {
    text: Option<String>,
    persona_id: Option<String>,
    duration_ms: u64,
}

impl AppState {
    pub fn bootstrap(
        app_handle: AppHandle,
        whisper_bin: PathBuf,
        backend_manifest_path: PathBuf,
    ) -> Result<Arc<Self>> {
        let base_dir = settings::app_support_dir()?;
        let settings_store = SettingsStore::new(app_handle.clone())?;
        let startup_settings = settings_store.get();

        let models_dir = settings::models_dir(&base_dir);
        std::fs::create_dir_all(&models_dir)
            .with_context(|| format!("failed creating model cache at {}", models_dir.display()))?;

        let recordings_dir = base_dir.join("recordings");
        std::fs::create_dir_all(&recordings_dir).with_context(|| {
            format!(
                "failed creating recordings directory at {}",
                recordings_dir.display()
            )
        })?;

        let backend = SpeechService::new(whisper_bin, backend_manifest_path, models_dir)?;

        let mut post_processor = PostProcessor::new(&base_dir).unwrap_or_else(|err| {
            eprintln!("failed to initialize post-processor, using fallback: {err:#}");
            PostProcessor::new_fallback()
        });
        post_processor.update_custom_dictionary(&startup_settings.post_process_custom_dictionary);

        let hotkey_config: &'static HotkeyConfig = Box::leak(Box::new(HotkeyConfig::new(
            &startup_settings.hotkey,
            startup_settings.dictation_mode,
        )));

        let usage_stats = UsageStatsStore::new(app_handle.clone());

        let state = Arc::new(Self {
            app_handle,
            instance_id: Uuid::new_v4().to_string(),
            settings_store,
            audio: AudioCapture::new(),
            backend,
            recordings_dir,
            dictation_state: Mutex::new(DictationState::Idle),
            recording_started_at: Mutex::new(None),
            active_trace_id: Mutex::new(None),
            transcript_logs: Mutex::new(Vec::new()),
            debug_logs: Mutex::new(Vec::new()),
            warmup_serial: Mutex::new(()),
            output_destination: Arc::new(atomic::AtomicU8::new(
                startup_settings.output_destination.as_u8(),
            )),
            active_downloads: Mutex::new(HashMap::new()),
            hotkey_config,
            last_injected_trailing_char: Mutex::new(None),
            last_injected_at_ms: Mutex::new(None),
            post_processor: Mutex::new(post_processor),
            llm_guard: llm_guard::LlmGuard::new(),
            pipeline_metrics: Mutex::new(PipelineMetrics::default()),
            usage_stats,
        });

        let startup_capture_tuning = capture_tuning(&startup_settings);
        if let Err(err) = state.audio.configure_capture(
            startup_settings.mic_device_id.as_deref(),
            startup_settings.keep_mic_stream_open,
            startup_capture_tuning,
        ) {
            eprintln!("failed initializing audio capture configuration: {err:#}");
        }

        state.debug_trace(
            "lifecycle",
            format!(
                "boot pid={} debug_logging={}",
                std::process::id(),
                startup_settings.debug_logging
            ),
        );
        state.clone().start_retention_janitor();

        Ok(state)
    }

    pub fn settings_store(&self) -> &SettingsStore {
        &self.settings_store
    }

    pub fn hotkey_config(&self) -> &'static HotkeyConfig {
        self.hotkey_config
    }

    /// Run the post-processing pipeline on text (for testing / preview).
    pub fn test_post_process(&self, text: &str) -> Result<crate::post_process::PipelineResult> {
        let settings = self.settings_store.get();
        self.post_processor.lock().run(text, &settings)
    }

    /// Update custom dictionary words in the post-processor.
    pub fn update_post_process_dictionary(&self, words: &[String]) {
        self.post_processor.lock().update_custom_dictionary(words);
    }

    pub fn backend(&self) -> &SpeechService {
        &self.backend
    }

    pub fn backend_status(&self) -> BackendStatus {
        let settings = self
            .settings_with_resolved_speech_api_key(&self.settings_store.get())
            .unwrap_or_else(|_| self.settings_store.get());
        self.backend.backend_status(&settings)
    }

    pub fn start_model_download(self: &Arc<Self>, model_id: String) -> Result<(), String> {
        {
            let downloads = self.active_downloads.lock();
            if downloads.contains_key(&model_id) {
                return Err(format!("model '{model_id}' is already being downloaded"));
            }
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.active_downloads
            .lock()
            .insert(model_id.clone(), cancel_flag.clone());

        let state = self.clone();
        std::thread::spawn(move || {
            let result = state.backend.download_model_with_progress(
                &model_id,
                &state.app_handle,
                &cancel_flag,
            );

            state.active_downloads.lock().remove(&model_id);

            match result {
                Ok(_) => {
                    let _ = state.app_handle.emit(
                        "model-download-progress",
                        ModelDownloadProgress {
                            model_id,
                            downloaded_bytes: 0,
                            total_bytes: 0,
                            status: "completed".to_string(),
                            error: None,
                        },
                    );
                }
                Err(err) => {
                    let msg = err.to_string();
                    let status = if cancel_flag.load(Ordering::Relaxed) {
                        "cancelled"
                    } else {
                        "error"
                    };
                    let _ = state.app_handle.emit(
                        "model-download-progress",
                        ModelDownloadProgress {
                            model_id,
                            downloaded_bytes: 0,
                            total_bytes: 0,
                            status: status.to_string(),
                            error: Some(msg),
                        },
                    );
                }
            }
        });

        Ok(())
    }

    pub fn cancel_model_download(&self, model_id: &str) -> Result<(), String> {
        let downloads = self.active_downloads.lock();
        match downloads.get(model_id) {
            Some(cancel_flag) => {
                cancel_flag.store(true, Ordering::Relaxed);
                Ok(())
            }
            None => Err(format!("no active download for model '{model_id}'")),
        }
    }

    pub fn get_transcript_logs(&self) -> Vec<TranscriptLogEntry> {
        self.transcript_logs.lock().clone()
    }

    pub fn get_debug_logs(&self) -> Vec<DebugLogEntry> {
        self.debug_logs.lock().clone()
    }

    pub fn get_dictation_state(&self) -> DictationState {
        *self.dictation_state.lock()
    }

    pub fn audio_input_level_percent(&self) -> u8 {
        self.audio.input_level_percent()
    }

    pub fn clear_transcript_logs(&self) {
        self.transcript_logs.lock().clear();
        let _ = self.app_handle.emit("transcript-logs-cleared", ());
    }

    pub fn clear_transcript_logs_and_recordings(&self) {
        let logs = self.transcript_logs.lock().drain(..).collect::<Vec<_>>();
        for entry in &logs {
            if let Some(ref filename) = entry.recording_file {
                let path = self.recordings_dir.join(filename);
                let _ = std::fs::remove_file(&path);
            }
        }
        let _ = self.app_handle.emit("transcript-logs-cleared", ());
    }

    pub fn get_recording_audio(&self, filename: &str) -> Result<Vec<u8>> {
        // Reject path traversal attempts
        if filename.contains('/')
            || filename.contains('\\')
            || filename.contains("..")
            || filename.is_empty()
        {
            anyhow::bail!("invalid filename");
        }
        let path = self.recordings_dir.join(filename);
        if !path.exists() {
            anyhow::bail!("recording not found (may have been pruned by retention policy)");
        }
        std::fs::read(&path)
            .with_context(|| format!("failed to read recording: {}", path.display()))
    }

    pub fn clear_debug_logs(&self) {
        self.debug_logs.lock().clear();
        let _ = self.app_handle.emit("debug-logs-cleared", ());
    }

    pub fn get_pipeline_metrics(&self) -> PipelineMetricsSnapshot {
        let mut snapshot = self.pipeline_metrics.lock().snapshot.clone();
        let guard_status = self.llm_guard.status();
        snapshot.llm_circuit_open = guard_status.circuit_open;
        snapshot.llm_circuit_open_until_ms = guard_status.circuit_open_until_ms;
        snapshot
    }

    pub fn clear_pipeline_metrics(&self) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot = PipelineMetricsSnapshot::default();
        metrics.failures.clear();
    }

    pub fn purge_local_artifacts(&self) -> Result<()> {
        purge_recordings(&self.recordings_dir)?;
        self.clear_transcript_logs();
        self.clear_debug_logs();
        Ok(())
    }

    pub fn start_recording(self: &Arc<Self>) {
        let state = self.dictation_state.lock();
        if matches!(
            *state,
            DictationState::Recording | DictationState::Transcribing | DictationState::Injecting
        ) {
            return;
        }

        let settings = self.settings_store.get();
        let capture_tuning = capture_tuning(&settings);
        // Generate trace_id before recording so the WAV file can be named
        // after it, establishing a single correlation key across all artifacts.
        let trace_id = self.begin_trace();
        let result = self.audio.start_recording(
            &self.recordings_dir,
            &trace_id,
            settings.mic_device_id.as_deref(),
            settings.keep_mic_stream_open,
            capture_tuning,
        );

        match result {
            Ok(_) => {
                self.metrics_increment_started();
                *self.recording_started_at.lock() = Some(Instant::now());
                drop(state);
                self.debug_trace_with_trace(
                    "trace",
                    format!(
                        "start pid={} instance_id={}",
                        std::process::id(),
                        self.instance_id
                    ),
                    Some(trace_id),
                );
                self.emit_partial_transcript(String::new());
                self.set_state(DictationState::Recording);
            }
            Err(err) => {
                eprintln!("failed starting recording: {err:#}");
                self.push_error_log(
                    trace_id,
                    settings.active_speech_model_id(),
                    format!("failed starting recording: {err:#}"),
                );
                drop(state);
                self.set_state(DictationState::Error);
            }
        }
    }

    pub fn stop_and_transcribe(self: Arc<Self>) {
        {
            let mut state = self.dictation_state.lock();
            if !matches!(*state, DictationState::Recording) {
                return;
            }
            *state = DictationState::Transcribing;
        }

        let trace_id = self.ensure_active_trace_id();
        let recording_duration_ms = self
            .recording_started_at
            .lock()
            .take()
            .map(|started_at| started_at.elapsed().as_millis() as u64)
            .unwrap_or(0);
        self.set_state(DictationState::Transcribing);

        std::thread::spawn(move || {
            let audio_path = match self.audio.stop_recording() {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("failed stopping recording: {err:#}");
                    let settings = self.settings_store.get();
                    self.push_error_log(
                        trace_id.clone(),
                        settings.active_speech_model_id(),
                        format!("failed stopping recording: {err:#}"),
                    );
                    self.metrics_increment_failed_dictation("audio", "STOP_RECORDING_FAILED");
                    self.finish_trace();
                    self.emit_partial_transcript(String::new());
                    self.set_state(DictationState::Error);
                    return;
                }
            };

            let recording_file = audio_path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned());

            let settings = self.settings_store.get();
            let resolved_settings = match self.settings_with_resolved_speech_api_key(&settings) {
                Ok(settings) => settings,
                Err(err) => {
                    self.push_error_log(
                        trace_id.clone(),
                        settings.active_speech_model_id(),
                        format!("failed resolving speech API key: {err:#}"),
                    );
                    self.metrics_increment_failed_dictation(
                        "transcribe",
                        "REMOTE_PROVIDER_UNAVAILABLE",
                    );
                    self.finish_trace();
                    self.emit_partial_transcript(String::new());
                    self.set_state(DictationState::Error);
                    return;
                }
            };
            let request = TranscribeRequest {
                request_id: trace_id.clone(),
                audio_path,
                model_id: resolved_settings.active_speech_model_id(),
                language: resolved_settings.whisper_language.clone(),
                compute_mode: resolved_settings.compute_mode,
                beam_size: resolved_settings.whisper_beam_size,
                prompt: resolved_settings.whisper_prompt.clone(),
                no_speech_thold: resolved_settings.whisper_no_speech_thold,
                temperature: resolved_settings.whisper_temperature,
                temperature_inc: resolved_settings.whisper_temperature_inc,
                threads: resolved_settings.whisper_threads,
            };
            self.debug_trace(
                "transcribe",
                format!(
                    "request_id={} model_id={} provider={} compute_mode={} audio_path={}",
                    request.request_id,
                    request.model_id,
                    resolved_settings.speech_provider.as_str(),
                    request.compute_mode.as_str(),
                    request.audio_path.display()
                ),
            );

            match self.backend.transcribe(&resolved_settings, &request) {
                Ok(response) => {
                    if let Some(code) = response.error_code {
                        if code == "AUDIO_SILENT" {
                            eprintln!(
                                "whisper backend skipped silent audio: {}",
                                response.error_message.unwrap_or_default()
                            );
                            self.finish_trace();
                            self.emit_partial_transcript(String::new());
                            self.set_state(DictationState::Idle);
                            return;
                        }

                        let error_message = response.error_message.unwrap_or_default();
                        eprintln!(
                            "whisper backend transcription error: {} ({})",
                            code, error_message
                        );
                        self.push_error_log(
                            response.request_id,
                            request.model_id.clone(),
                            format!(
                                "whisper backend transcription error: {code} ({error_message})"
                            ),
                        );
                        self.metrics_increment_failed_dictation("transcribe", &code);
                        self.finish_trace();
                        self.emit_partial_transcript(String::new());
                        self.set_state(DictationState::Error);
                        return;
                    }

                    let response_text = normalize_transcript_text(&response.text);
                    self.debug_trace(
                        "transcribe",
                        format!(
                            "response request_id={} backend={:?} duration_ms={} text_len={} text_preview=\"{}\"",
                            response.request_id,
                            response.backend,
                            response.duration_ms,
                            response_text.len(),
                            self.debug_preview_for_log(&response_text, 280)
                        ),
                    );
                    if let Err(err) = self.complete_transcription(
                        response.request_id,
                        request.model_id,
                        response.backend,
                        recording_duration_ms,
                        response.duration_ms,
                        response_text,
                        recording_file.clone(),
                    ) {
                        eprintln!("text insertion failed: {err:#}");
                        self.metrics_increment_failed_dictation("inject", "TEXT_INSERTION_FAILED");
                        self.finish_trace();
                        self.set_state(DictationState::Error);
                    }
                }
                Err(err) => {
                    eprintln!("transcription failed: {err:#}");
                    self.push_error_log(
                        request.request_id,
                        request.model_id,
                        format!("transcription failed: {err:#}"),
                    );
                    self.metrics_increment_failed_dictation("transcribe", "TRANSCRIBE_FAILED");
                    self.finish_trace();
                    self.emit_partial_transcript(String::new());
                    self.set_state(DictationState::Error);
                }
            }
        });
    }

    pub fn set_state(&self, state: DictationState) {
        *self.dictation_state.lock() = state;
        self.emit_state(state);
        self.apply_tray_icon_state(state);
    }

    fn debug_logging_enabled(&self) -> bool {
        self.settings_store.get().debug_logging
    }

    fn debug_trace(&self, scope: &str, message: impl Into<String>) {
        let trace_id = self.active_trace_id.lock().clone();
        self.debug_trace_with_trace(scope, message, trace_id);
    }

    fn debug_trace_with_trace(
        &self,
        scope: &str,
        message: impl Into<String>,
        trace_id: Option<String>,
    ) {
        if !self.debug_logging_enabled() {
            return;
        }

        let message = message.into();
        match trace_id.as_deref() {
            Some(trace_id) => {
                eprintln!(
                    "[debug:{scope}] instance_id={} trace_id={} {}",
                    self.instance_id, trace_id, message
                );
            }
            None => {
                eprintln!(
                    "[debug:{scope}] instance_id={} {}",
                    self.instance_id, message
                );
            }
        }
        self.push_debug_log(scope, message, trace_id);
    }

    fn begin_trace(&self) -> String {
        let trace_id = Uuid::new_v4().to_string();
        *self.active_trace_id.lock() = Some(trace_id.clone());
        trace_id
    }

    fn ensure_active_trace_id(&self) -> String {
        let mut active_trace_id = self.active_trace_id.lock();
        match active_trace_id.clone() {
            Some(existing) => existing,
            None => {
                let trace_id = Uuid::new_v4().to_string();
                *active_trace_id = Some(trace_id.clone());
                trace_id
            }
        }
    }

    fn finish_trace(&self) {
        let trace_id = self.active_trace_id.lock().take();
        if let Some(trace_id) = trace_id {
            self.debug_trace_with_trace("trace", "end", Some(trace_id));
        }
    }

    pub fn preflight_permissions(&self) {
        permissions_macos::ensure_preflight_status(&self.app_handle);
    }

    /// Check whether the saved `mic_device_id` still exists in the system
    /// device list.  If the device has disappeared, reset the setting to
    /// `None` (system default), apply the runtime audio change, and emit a
    /// `mic-device-reset` event so the frontend can update.
    pub fn reconcile_mic_if_stale(self: &Arc<Self>) {
        let current = self.settings_store.get();
        let mic_id = match &current.mic_device_id {
            Some(id) => id.clone(),
            None => return, // already on system default
        };

        let devices = match crate::audio::AudioCapture::list_input_devices() {
            Ok(d) => d,
            Err(err) => {
                eprintln!("mic reconciliation: failed listing devices: {err:#}");
                return;
            }
        };

        if devices.iter().any(|d| d.id == mic_id) {
            return; // device still present
        }

        eprintln!(
            "mic reconciliation: device '{}' no longer available, reverting to system default",
            mic_id
        );

        let patch = settings::SettingsPatch {
            mic_device_id: Some(None),
            ..Default::default()
        };

        match self.settings_store.update(patch) {
            Ok(next) => {
                self.clone().apply_runtime_settings(&current, &next);
                let _ = self.app_handle.emit("mic-device-reset", &next);
            }
            Err(err) => {
                eprintln!("mic reconciliation: failed updating settings: {err:#}");
            }
        }
    }

    pub fn apply_runtime_settings(self: Arc<Self>, previous: &Settings, next: &Settings) {
        if !previous.debug_logging && next.debug_logging {
            self.debug_trace(
                "lifecycle",
                format!(
                    "debug logging enabled at runtime pid={}",
                    std::process::id()
                ),
            );
        }

        if previous.hotkey != next.hotkey || previous.dictation_mode != next.dictation_mode {
            self.hotkey_config.update(&next.hotkey, next.dictation_mode);
            let spec = next.hotkey.spec();
            self.debug_trace(
                "settings",
                format!(
                    "hotkey config updated: key={} (keycode={}) mode={:?}",
                    spec.display_label, spec.keycode, next.dictation_mode
                ),
            );
        }

        if previous.output_destination != next.output_destination {
            self.output_destination
                .store(next.output_destination.as_u8(), Ordering::Relaxed);
            self.debug_trace(
                "settings",
                format!(
                    "output_destination changed {:?} -> {:?}",
                    previous.output_destination, next.output_destination
                ),
            );
        }

        if previous.keep_mic_stream_open != next.keep_mic_stream_open
            || previous.mic_device_id != next.mic_device_id
            || previous.audio_channel_mode != next.audio_channel_mode
            || previous.input_gain_db != next.input_gain_db
            || previous.high_pass_filter != next.high_pass_filter
        {
            let capture_tuning = capture_tuning(next);
            if let Err(err) = self.audio.configure_capture(
                next.mic_device_id.as_deref(),
                next.keep_mic_stream_open,
                capture_tuning,
            ) {
                eprintln!("failed applying runtime audio settings: {err:#}");
            }
        }

        if (previous.model_id != next.model_id
            || previous.compute_mode != next.compute_mode
            || previous.speech_provider != next.speech_provider)
            && next.speech_provider.is_local()
        {
            self.clone().schedule_backend_warmup(
                next.model_id.clone(),
                next.compute_mode,
                "settings-change",
            );
        }

        if previous.post_process_custom_dictionary != next.post_process_custom_dictionary {
            self.post_processor
                .lock()
                .update_custom_dictionary(&next.post_process_custom_dictionary);
        }
    }

    pub fn schedule_startup_warmup(self: Arc<Self>) {
        let settings = self.settings_store.get();
        if settings.speech_provider.is_local() {
            self.schedule_backend_warmup(settings.model_id, settings.compute_mode, "startup");
        }
    }

    pub fn schedule_backend_warmup(
        self: Arc<Self>,
        model_id: String,
        compute_mode: ComputeMode,
        reason: &'static str,
    ) {
        std::thread::spawn(move || {
            let _serial = self.warmup_serial.lock();
            eprintln!(
                "starting backend warm-up ({reason}) for model '{}' using {}",
                model_id,
                compute_mode.as_str()
            );
            if let Err(err) = self.backend.warm_up(&model_id, compute_mode) {
                eprintln!("backend warm-up failed: {err:#}");
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_transcription(
        &self,
        request_id: String,
        model_id: String,
        backend: Option<String>,
        recording_duration_ms: u64,
        transcription_duration_ms: u64,
        text: String,
        recording_file: Option<String>,
    ) -> Result<()> {
        let settings = self.settings_store.get();
        let request_id_for_log = request_id.clone();
        let dest_snapshot = settings.output_destination;
        let dest_runtime =
            OutputDestination::from_u8(self.output_destination.load(Ordering::Relaxed));
        if dest_snapshot != dest_runtime {
            self.debug_trace(
                "output",
                format!(
                    "request_id={} gate_sync snapshot={:?} runtime={:?} -> {:?}",
                    request_id_for_log, dest_snapshot, dest_runtime, dest_snapshot
                ),
            );
            self.output_destination
                .store(dest_snapshot.as_u8(), Ordering::Relaxed);
        }

        let cleanup_requested = settings.beta_ai_enhancement_enabled
            && settings.llm_cleanup_enabled
            && !settings.llm_cleanup_model.trim().is_empty()
            && !settings.llm_cleanup_base_url.trim().is_empty();
        let raw_text = text;
        let mut final_text = raw_text.clone();
        self.debug_trace(
            "transcription",
            format!(
                "request_id={} start backend={:?} recording_ms={} transcription_ms={} cleanup_requested={} raw_len={} raw_preview=\"{}\"",
                request_id_for_log,
                backend,
                recording_duration_ms,
                transcription_duration_ms,
                cleanup_requested,
                raw_text.len(),
                self.debug_preview_for_log(&raw_text, 280)
            ),
        );
        self.debug_trace(
            "output",
            format!(
                "request_id={} policy dest_snapshot={:?} dest_runtime={:?} cleanup_requested={}",
                request_id_for_log,
                dest_snapshot,
                OutputDestination::from_u8(self.output_destination.load(Ordering::Relaxed)),
                cleanup_requested
            ),
        );

        let post_process_outcome =
            self.run_post_process_stage(&request_id_for_log, &raw_text, &settings);
        if let Some(output) = post_process_outcome.output.clone() {
            final_text = output;
        }
        let post_process_duration_ms = post_process_outcome.duration_ms;
        let post_process_edits_applied = post_process_outcome.edits_applied;
        let post_process_edits_rejected = post_process_outcome.edits_rejected;
        let post_process_text = post_process_outcome.text_for_log;

        let mut cleanup_roundtrip_duration_ms = 0u64;

        if cleanup_requested && !raw_text.trim().is_empty() {
            // Ensure stepper shows "Processing" (step 2) during LLM cleanup.
            // If post-processing was skipped (disabled / empty text) the state
            // is still Transcribing; advance it so the UI doesn't regress.
            self.set_state(DictationState::PostProcessing);
            let cleanup_outcome =
                self.run_llm_cleanup_stage(&request_id_for_log, &raw_text, &settings);
            if let Some(output) = cleanup_outcome.output {
                final_text = output;
            }
            cleanup_roundtrip_duration_ms = cleanup_outcome.roundtrip_duration_ms;
        }

        // Content classification stage
        let classification_outcome =
            self.run_classification_stage(&request_id_for_log, &final_text, &settings);

        // Persona transform stage
        let persona_outcome = self.run_persona_stage(
            &request_id_for_log,
            &final_text,
            classification_outcome.result.as_ref(),
            &settings,
        );

        self.finish_complete_transcription(
            request_id,
            model_id,
            backend,
            recording_duration_ms,
            transcription_duration_ms,
            cleanup_requested,
            raw_text,
            final_text,
            cleanup_roundtrip_duration_ms,
            post_process_duration_ms,
            post_process_edits_applied,
            post_process_edits_rejected,
            post_process_text,
            recording_file,
            request_id_for_log,
            classification_outcome,
            persona_outcome,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_complete_transcription(
        &self,
        request_id: String,
        model_id: String,
        backend: Option<String>,
        recording_duration_ms: u64,
        transcription_duration_ms: u64,
        cleanup_requested: bool,
        raw_text: String,
        final_text: String,
        cleanup_roundtrip_duration_ms: u64,
        post_process_duration_ms: u64,
        post_process_edits_applied: u32,
        post_process_edits_rejected: u32,
        post_process_text: Option<String>,
        recording_file: Option<String>,
        request_id_for_log: String,
        classification_outcome: ClassificationStageOutcome,
        persona_outcome: PersonaStageOutcome,
    ) -> Result<()> {
        // Defensive trim: whisper.cpp tokens are space-prefixed and pipeline
        // stages may preserve leading/trailing whitespace.
        let final_text = final_text.trim().to_string();

        let total_waterfall_duration_ms = recording_duration_ms
            .saturating_add(transcription_duration_ms)
            .saturating_add(post_process_duration_ms)
            .saturating_add(cleanup_roundtrip_duration_ms)
            .saturating_add(classification_outcome.duration_ms)
            .saturating_add(persona_outcome.duration_ms);
        self.metrics_observe_stage_latency("recording", recording_duration_ms);
        self.metrics_observe_stage_latency("transcription", transcription_duration_ms);
        if post_process_duration_ms > 0 {
            self.metrics_observe_stage_latency("post_process", post_process_duration_ms);
        }
        if cleanup_roundtrip_duration_ms > 0 {
            self.metrics_observe_stage_latency("llm_cleanup", cleanup_roundtrip_duration_ms);
        }
        if classification_outcome.duration_ms > 0 {
            self.metrics_observe_stage_latency(
                "classification",
                classification_outcome.duration_ms,
            );
        }
        if persona_outcome.duration_ms > 0 {
            self.metrics_observe_stage_latency("persona", persona_outcome.duration_ms);
        }
        let cleanup_applied = cleanup_requested && final_text != raw_text;
        let classification_blocked = classification_outcome
            .result
            .as_ref()
            .is_some_and(|r| r.blocked);
        self.debug_trace(
            "transcription",
            format!(
                "request_id={} finalize cleanup_applied={} cleanup_roundtrip_ms={} classification_blocked={} total_waterfall_ms={} final_len={} final_preview=\"{}\"",
                request_id_for_log,
                cleanup_applied,
                cleanup_roundtrip_duration_ms,
                classification_blocked,
                total_waterfall_duration_ms,
                final_text.len(),
                self.debug_preview_for_log(&final_text, 280)
            ),
        );
        let request_id_for_injection = request_id_for_log.clone();
        self.push_transcript_log(TranscriptLogEntry {
            instance_id: self.instance_id.clone(),
            trace_id: request_id_for_log.clone(),
            request_id,
            timestamp_ms: current_timestamp_ms(),
            model_id,
            backend,
            duration_ms: transcription_duration_ms,
            recording_duration_ms,
            transcription_duration_ms,
            cleanup_roundtrip_duration_ms,
            total_waterfall_duration_ms,
            text: final_text.clone(),
            raw_text: Some(raw_text),
            post_process_text,
            is_final: true,
            cleanup_requested,
            cleanup_applied,
            post_process_duration_ms,
            post_process_edits_applied,
            post_process_edits_rejected,
            recording_file,
            classification_result: classification_outcome.result,
            classification_duration_ms: classification_outcome.duration_ms,
            persona_id: persona_outcome.persona_id,
            persona_text: persona_outcome.text,
            persona_duration_ms: persona_outcome.duration_ms,
        });

        // If content classification blocked the text, skip injection.
        if classification_blocked {
            self.debug_trace(
                "output",
                format!(
                    "request_id={} skipped injection: content classification blocked",
                    request_id_for_injection
                ),
            );
            self.emit_partial_transcript(String::new());
            self.metrics_increment_succeeded();
            self.finish_trace();
            self.set_state(DictationState::Idle);
            return Ok(());
        }

        let current_dest =
            OutputDestination::from_u8(self.output_destination.load(Ordering::Relaxed));
        match current_dest {
            OutputDestination::Input => {
                self.set_state(DictationState::Injecting);
                let injectable = self.prepare_injectable_text(&final_text);
                self.debug_trace(
                    "output",
                    format!(
                        "request_id={} inject text_len={} text_preview=\"{}\"",
                        request_id_for_injection,
                        injectable.len(),
                        self.debug_preview_for_log(&injectable, 280)
                    ),
                );
                self.inject_text_on_main_thread(&request_id_for_injection, injectable.clone())?;
                if let Some(ch) = injectable.chars().last() {
                    *self.last_injected_trailing_char.lock() = Some(ch);
                    *self.last_injected_at_ms.lock() = Some(current_timestamp_ms());
                }
            }
            OutputDestination::Clipboard => {
                self.set_state(DictationState::Injecting);
                self.debug_trace(
                    "output",
                    format!(
                        "request_id={} clipboard text_len={} text_preview=\"{}\"",
                        request_id_for_injection,
                        final_text.len(),
                        self.debug_preview_for_log(&final_text, 280)
                    ),
                );
                self.copy_to_clipboard(&final_text);
            }
            OutputDestination::None => {
                self.debug_trace(
                    "output",
                    format!(
                        "request_id={} skipped output_destination=none",
                        request_id_for_injection
                    ),
                );
            }
        }

        self.emit_partial_transcript(String::new());
        self.metrics_increment_succeeded();
        self.finish_trace();
        self.set_state(DictationState::Idle);
        Ok(())
    }

    fn run_post_process_stage(
        &self,
        request_id_for_log: &str,
        raw_text: &str,
        settings: &Settings,
    ) -> PostProcessStageOutcome {
        let mut outcome = PostProcessStageOutcome::default();
        if !settings.post_process_enabled || raw_text.trim().is_empty() {
            return outcome;
        }

        self.set_state(DictationState::PostProcessing);
        self.metrics_increment_pp_runs();
        let pp_start = Instant::now();
        match self.post_processor.lock().run(raw_text, settings) {
            Ok(result) => {
                self.debug_trace(
                    "post_process",
                    format!(
                        "request_id={} applied={} rejected={} duration_ms={} output_preview=\"{}\"",
                        request_id_for_log,
                        result.applied_edits.len(),
                        result.rejected_edits.len(),
                        result.total_duration_ms,
                        self.debug_preview_for_log(&result.output, 280)
                    ),
                );
                outcome.edits_applied = result.applied_edits.len() as u32;
                outcome.edits_rejected = result.rejected_edits.len() as u32;
                self.metrics_add_pp_edits(outcome.edits_applied, outcome.edits_rejected);
                if !result.output.is_empty() {
                    outcome.output = Some(result.output.clone());
                    outcome.text_for_log = Some(result.output);
                }
            }
            Err(err) => {
                eprintln!(
                    "post-processing failed for request {}: {err:#}",
                    request_id_for_log
                );
                self.metrics_record_failure_event("post_process", "POST_PROCESS_FAILED");
            }
        }
        // Use ceiling so sub-millisecond post-processing still reports ≥1 ms
        // instead of truncating to 0 (which hides the bar in the pipeline chart).
        let elapsed_us = pp_start.elapsed().as_micros() as u64;
        outcome.duration_ms = elapsed_us.div_ceil(1000).max(1);
        outcome
    }

    fn run_llm_cleanup_stage(
        &self,
        request_id_for_log: &str,
        raw_text: &str,
        settings: &Settings,
    ) -> LlmCleanupStageOutcome {
        let mut outcome = LlmCleanupStageOutcome::default();
        let llm_settings = match self.settings_with_resolved_llm_api_key(settings) {
            Ok(v) => v,
            Err(err) => {
                self.metrics_record_failure_event("llm_cleanup", "LLM_SECRET_UNAVAILABLE");
                eprintln!("llm cleanup skipped due to Keychain error: {err:#}");
                return outcome;
            }
        };
        self.debug_trace(
            "llm_cleanup",
            format!(
                "request_id={} dispatch model={} base_url={} raw_len={} raw_preview=\"{}\"",
                request_id_for_log,
                llm_settings.llm_cleanup_model,
                llm_settings.llm_cleanup_base_url,
                raw_text.len(),
                self.debug_preview_for_log(raw_text, 280)
            ),
        );

        let cleanup_roundtrip_started_at = Instant::now();
        let trace_request_id = request_id_for_log.to_string();
        let guard_outcome = self.llm_guard.execute(|timeout| {
            llm_cleanup::cleanup_text_with_trace_timeout(&llm_settings, raw_text, timeout, |line| {
                self.debug_trace(
                    "llm_cleanup",
                    format!("request_id={} {line}", trace_request_id),
                );
            })
        });

        self.metrics_add_llm_attempts(guard_outcome.attempts as u64);
        if let Some(code) = guard_outcome.error_code {
            self.metrics_increment_llm_fail(code);
            self.metrics_record_failure_event("llm_cleanup", code.as_str());
            self.debug_trace(
                "llm_cleanup",
                format!(
                    "request_id={} outcome=error code={} attempts={} circuit_open={}",
                    request_id_for_log,
                    code.as_str(),
                    guard_outcome.attempts,
                    guard_outcome.circuit_open
                ),
            );
        } else {
            self.metrics_increment_llm_success();
        }

        if let Some(cleaned) = guard_outcome.value {
            let normalized_cleaned = normalize_transcript_text(&cleaned);
            self.debug_trace(
                "llm_cleanup",
                format!(
                    "request_id={} cleaned_len={} cleaned_preview=\"{}\"",
                    request_id_for_log,
                    normalized_cleaned.len(),
                    self.debug_preview_for_log(&normalized_cleaned, 280)
                ),
            );
            if !normalized_cleaned.is_empty() {
                outcome.output = Some(normalized_cleaned);
            }
        }

        let cleanup_elapsed_us = cleanup_roundtrip_started_at.elapsed().as_micros() as u64;
        outcome.roundtrip_duration_ms = cleanup_elapsed_us.div_ceil(1000).max(1);
        self.debug_trace(
            "llm_cleanup",
            format!(
                "request_id={} roundtrip_ms={} attempts={} circuit_open={} open_until_ms={:?}",
                request_id_for_log,
                outcome.roundtrip_duration_ms,
                guard_outcome.attempts,
                guard_outcome.circuit_open,
                guard_outcome.circuit_open_until_ms
            ),
        );
        outcome
    }

    fn run_classification_stage(
        &self,
        request_id_for_log: &str,
        text: &str,
        settings: &Settings,
    ) -> ClassificationStageOutcome {
        let mut outcome = ClassificationStageOutcome::default();
        if !settings.beta_content_classification_enabled
            || !settings.content_classification_enabled
            || !settings.content_classification_auto_apply
            || text.trim().is_empty()
        {
            return outcome;
        }
        let llm_settings = match self.settings_with_resolved_llm_api_key(settings) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("classification skipped due to Keychain error: {err:#}");
                return outcome;
            }
        };
        self.debug_trace(
            "classification",
            format!(
                "request_id={} dispatch text_len={}",
                request_id_for_log,
                text.len()
            ),
        );
        let started_at = Instant::now();
        match content_classification::classify_text(
            &llm_settings,
            text,
            Duration::from_secs(30),
            |line| {
                self.debug_trace(
                    "classification",
                    format!("request_id={} {line}", request_id_for_log),
                );
            },
        ) {
            Ok(result) => {
                self.debug_trace(
                    "classification",
                    format!(
                        "request_id={} score={:.2} blocked={} warning={} categories={}",
                        request_id_for_log,
                        result.score,
                        result.blocked,
                        result.warning,
                        result.categories.len()
                    ),
                );
                outcome.result = Some(result);
            }
            Err(err) => {
                eprintln!(
                    "classification failed for request {}: {err:#}",
                    request_id_for_log
                );
                self.metrics_record_failure_event("classification", "CLASSIFICATION_FAILED");
            }
        }
        let elapsed_us = started_at.elapsed().as_micros() as u64;
        outcome.duration_ms = elapsed_us.div_ceil(1000).max(1);
        outcome
    }

    fn run_persona_stage(
        &self,
        request_id_for_log: &str,
        text: &str,
        classification: Option<&ContentClassificationResult>,
        settings: &Settings,
    ) -> PersonaStageOutcome {
        let mut outcome = PersonaStageOutcome::default();
        if !settings.beta_personas_enabled
            || !settings.persona_enabled
            || !settings.persona_auto_apply
            || text.trim().is_empty()
        {
            return outcome;
        }
        let active_persona = settings
            .personas
            .iter()
            .find(|p| p.id == settings.persona_active_id);
        let active_persona = match active_persona {
            Some(p) => p,
            None => {
                self.debug_trace(
                    "persona",
                    format!(
                        "request_id={} skipped: active persona '{}' not found",
                        request_id_for_log, settings.persona_active_id
                    ),
                );
                return outcome;
            }
        };
        let llm_settings = match self.settings_with_resolved_llm_api_key(settings) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("persona skipped due to Keychain error: {err:#}");
                return outcome;
            }
        };
        self.debug_trace(
            "persona",
            format!(
                "request_id={} dispatch persona='{}' text_len={}",
                request_id_for_log,
                active_persona.name,
                text.len()
            ),
        );
        outcome.persona_id = Some(active_persona.id.clone());
        let started_at = Instant::now();
        match persona::transform_text(
            &llm_settings,
            active_persona,
            text,
            classification,
            Duration::from_secs(60),
            |line| {
                self.debug_trace(
                    "persona",
                    format!("request_id={} {line}", request_id_for_log),
                );
            },
        ) {
            Ok(transformed) => {
                let trimmed = transformed.trim().to_string();
                self.debug_trace(
                    "persona",
                    format!(
                        "request_id={} transformed_len={} preview=\"{}\"",
                        request_id_for_log,
                        trimmed.len(),
                        self.debug_preview_for_log(&trimmed, 280)
                    ),
                );
                if !trimmed.is_empty() {
                    outcome.text = Some(trimmed);
                }
            }
            Err(err) => {
                eprintln!(
                    "persona transform failed for request {}: {err:#}",
                    request_id_for_log
                );
                self.metrics_record_failure_event("persona", "PERSONA_FAILED");
            }
        }
        let elapsed_us = started_at.elapsed().as_micros() as u64;
        outcome.duration_ms = elapsed_us.div_ceil(1000).max(1);
        outcome
    }

    /// On-demand persona transform for the studio UI.
    pub fn transform_with_persona(&self, text: &str, persona_id: &str) -> Result<String> {
        let settings = self.settings_store.get();
        if !settings.beta_personas_enabled {
            anyhow::bail!("personas beta flag is disabled");
        }
        let persona = settings
            .personas
            .iter()
            .find(|p| p.id == persona_id)
            .ok_or_else(|| anyhow::anyhow!("persona not found: {persona_id}"))?
            .clone();
        let llm_settings = self.settings_with_resolved_llm_api_key(&settings)?;
        persona::transform_text(
            &llm_settings,
            &persona,
            text,
            None,
            Duration::from_secs(60),
            |_| {},
        )
        .map_err(|e| anyhow::anyhow!("{e}"))
    }

    fn settings_with_resolved_llm_api_key(&self, settings: &Settings) -> Result<Settings> {
        let mut resolved = settings.clone();
        resolved.llm_cleanup_api_key = secrets::load_llm_api_key()?.unwrap_or_default();
        resolved.llm_cleanup_api_key_configured = !resolved.llm_cleanup_api_key.is_empty();
        Ok(resolved)
    }

    pub fn settings_with_resolved_speech_api_key(&self, settings: &Settings) -> Result<Settings> {
        let mut resolved = settings.clone();
        resolved.speech_remote_api_key = secrets::load_speech_api_key()?.unwrap_or_default();
        resolved.speech_remote_api_key_configured = !resolved.speech_remote_api_key.is_empty();
        Ok(resolved)
    }

    fn debug_preview_for_log(&self, text: &str, max_chars: usize) -> String {
        if self.settings_store.get().debug_log_include_content {
            debug_preview(text, max_chars)
        } else {
            format!("[redacted len={} sha256={}]", text.len(), hash_text(text))
        }
    }

    fn metrics_increment_started(&self) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.dictations_started = metrics.snapshot.dictations_started.saturating_add(1);
    }

    fn metrics_increment_succeeded(&self) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.dictations_succeeded =
            metrics.snapshot.dictations_succeeded.saturating_add(1);
    }

    fn metrics_increment_failed_dictation(&self, stage: &str, error_code: &str) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.dictations_failed = metrics.snapshot.dictations_failed.saturating_add(1);
        drop(metrics);
        self.metrics_record_failure_event(stage, error_code);
    }

    fn metrics_record_failure_event(&self, stage: &str, error_code: &str) {
        let mut metrics = self.pipeline_metrics.lock();
        let event = PipelineFailureEvent {
            timestamp_ms: current_timestamp_ms(),
            stage: stage.to_string(),
            error_code: error_code.to_string(),
        };
        metrics.failures.push_back(event);
        while metrics.failures.len() > 100 {
            metrics.failures.pop_front();
        }
        metrics.snapshot.last_100_failures = metrics.failures.iter().cloned().collect();
    }

    fn metrics_increment_pp_runs(&self) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.pp_runs = metrics.snapshot.pp_runs.saturating_add(1);
    }

    fn metrics_add_pp_edits(&self, applied: u32, rejected: u32) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.pp_edits_applied_total = metrics
            .snapshot
            .pp_edits_applied_total
            .saturating_add(applied as u64);
        metrics.snapshot.pp_edits_rejected_total = metrics
            .snapshot
            .pp_edits_rejected_total
            .saturating_add(rejected as u64);
    }

    fn metrics_add_llm_attempts(&self, attempts: u64) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.llm_attempts = metrics.snapshot.llm_attempts.saturating_add(attempts);
    }

    fn metrics_increment_llm_success(&self) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.llm_success = metrics.snapshot.llm_success.saturating_add(1);
    }

    fn metrics_increment_llm_fail(&self, code: llm_guard::LlmGuardErrorCode) {
        let mut metrics = self.pipeline_metrics.lock();
        metrics.snapshot.llm_fail = metrics.snapshot.llm_fail.saturating_add(1);
        if matches!(code, llm_guard::LlmGuardErrorCode::Timeout) {
            metrics.snapshot.llm_timeout = metrics.snapshot.llm_timeout.saturating_add(1);
        }
        if matches!(code, llm_guard::LlmGuardErrorCode::CircuitOpen) {
            metrics.snapshot.llm_skipped_circuit_open =
                metrics.snapshot.llm_skipped_circuit_open.saturating_add(1);
        }
    }

    fn metrics_observe_stage_latency(&self, stage: &str, duration_ms: u64) {
        let mut metrics = self.pipeline_metrics.lock();
        let histogram = metrics
            .snapshot
            .stage_latency_histograms
            .entry(stage.to_string())
            .or_default();
        histogram.observe(duration_ms);
    }

    fn start_retention_janitor(self: Arc<Self>) {
        std::thread::spawn(move || loop {
            let retention = self
                .settings_store
                .get()
                .recording_retention_hours
                .clamp(1, 24 * 30);
            let retention_secs = (retention as u64).saturating_mul(3600);
            if let Err(err) = prune_recordings_older_than(
                &self.recordings_dir,
                Duration::from_secs(retention_secs),
            ) {
                eprintln!("recording retention janitor failed: {err:#}");
            }
            std::thread::sleep(Duration::from_secs(3600));
        });
    }

    fn emit_state(&self, state: DictationState) {
        let _ = self.app_handle.emit("dictation-state", state);
        if matches!(
            state,
            DictationState::Recording
                | DictationState::Transcribing
                | DictationState::PostProcessing
                | DictationState::Injecting
        ) {
            if let Some(window) = self.app_handle.get_webview_window("hud") {
                self.position_hud_window(&window);
                let _ = window.show();
            }
        }
    }

    fn position_hud_window<R: tauri::Runtime>(&self, window: &tauri::WebviewWindow<R>) {
        let monitor = window
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| window.primary_monitor().ok().flatten());
        let Some(monitor) = monitor else {
            return;
        };

        let outer_size = match window.outer_size() {
            Ok(size) => size,
            Err(_) => return,
        };

        let work_area = monitor.work_area();
        let work_x = work_area.position.x;
        let work_y = work_area.position.y;
        let work_w = i32::try_from(work_area.size.width).unwrap_or(i32::MAX);
        let work_h = i32::try_from(work_area.size.height).unwrap_or(i32::MAX);
        let hud_w = i32::try_from(outer_size.width).unwrap_or(i32::MAX);
        let hud_h = i32::try_from(outer_size.height).unwrap_or(i32::MAX);

        let side_margin = 12;
        let bottom_margin = 12;
        let x = work_x + ((work_w - hud_w) / 2).max(side_margin);
        let y = work_y + work_h - hud_h - bottom_margin;

        let _ = window.set_position(PhysicalPosition::new(x, y));
    }

    fn emit_partial_transcript(&self, text: String) {
        if text.is_empty() {
            self.debug_trace("partial_emit", "clear");
        } else {
            self.debug_trace(
                "partial_emit",
                format!(
                    "text_len={} text_preview=\"{}\"",
                    text.len(),
                    self.debug_preview_for_log(&text, 200)
                ),
            );
        }
        let _ = self.app_handle.emit("dictation-partial", text);
    }

    /// Prepend a space when consecutive dictations would otherwise be glued
    /// together.  Delegates to [`needs_inter_injection_space`] for the decision.
    fn prepare_injectable_text(&self, text: &str) -> String {
        let prev_trailing = *self.last_injected_trailing_char.lock();
        let last_injected_at_ms = *self.last_injected_at_ms.lock();
        let now_ms = current_timestamp_ms();
        if should_prepend_inter_injection_space(prev_trailing, last_injected_at_ms, now_ms, text) {
            format!(" {text}")
        } else {
            text.to_string()
        }
    }

    fn copy_to_clipboard(&self, text: &str) {
        use arboard::Clipboard;
        match Clipboard::new().and_then(|mut cb| cb.set_text(text.to_string())) {
            Ok(()) => self.debug_trace("output", "clipboard copy succeeded".to_string()),
            Err(err) => self.debug_trace("output", format!("clipboard copy failed: {err}")),
        }
    }

    fn inject_text_on_main_thread(&self, request_id: &str, text: String) -> Result<()> {
        let dest = OutputDestination::from_u8(self.output_destination.load(Ordering::Relaxed));
        if dest != OutputDestination::Input {
            self.debug_trace(
                "inject",
                format!(
                    "request_id={} skipped preflight output_destination={:?}",
                    request_id, dest
                ),
            );
            return Ok(());
        }

        let debug_enabled = self.debug_logging_enabled();
        let output_dest = self.output_destination.clone();
        let request_id_owned = request_id.to_string();
        let text_preview_for_log = self.debug_preview_for_log(&text, 280);
        let (tx, rx) = bounded::<std::result::Result<bool, String>>(1);
        self.app_handle
            .run_on_main_thread(move || {
                let dest = OutputDestination::from_u8(output_dest.load(Ordering::Relaxed));
                if dest != OutputDestination::Input {
                    if debug_enabled {
                        eprintln!(
                            "[debug:inject] request_id={} main_thread_skip output_destination={:?}",
                            request_id_owned, dest
                        );
                    }
                    let _ = tx.send(Ok(false));
                    return;
                }
                if debug_enabled {
                    eprintln!(
                        "[debug:inject] request_id={} main_thread_start text_len={} text_preview=\"{}\"",
                        request_id_owned,
                        text.len(),
                        text_preview_for_log
                    );
                }
                let result = text_inject_macos::inject_text(&text)
                    .or_else(|_| text_inject_macos::inject_with_clipboard_fallback(&text))
                    .map(|_| true)
                    .map_err(|err| err.to_string());
                let _ = tx.send(result);
            })
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;

        match rx.recv() {
            Ok(Ok(true)) => {
                self.debug_trace("inject", format!("request_id={} result=ok", request_id));
                Ok(())
            }
            Ok(Ok(false)) => {
                self.debug_trace(
                    "inject",
                    format!(
                        "request_id={} skipped main-thread output_destination!=input",
                        request_id
                    ),
                );
                Ok(())
            }
            Ok(Err(err)) => {
                self.debug_trace(
                    "inject",
                    format!("request_id={} result=error err={}", request_id, err),
                );
                Err(anyhow::anyhow!(err))
            }
            Err(err) => Err(anyhow::anyhow!(format!(
                "failed waiting for main-thread injection: {err}"
            ))),
        }
    }

    fn push_transcript_log(&self, entry: TranscriptLogEntry) {
        self.debug_trace(
            "transcript_log",
            format!(
                "add trace_id={} request_id={} final={} cleanup_requested={} cleanup_applied={} recording_ms={} transcription_ms={} cleanup_roundtrip_ms={} total_waterfall_ms={} text_len={} raw_len={} text_preview=\"{}\" raw_preview=\"{}\"",
                entry.trace_id,
                entry.request_id,
                entry.is_final,
                entry.cleanup_requested,
                entry.cleanup_applied,
                entry.recording_duration_ms,
                entry.transcription_duration_ms,
                entry.cleanup_roundtrip_duration_ms,
                entry.total_waterfall_duration_ms,
                entry.text.len(),
                entry.raw_text.as_ref().map_or(0, String::len),
                self.debug_preview_for_log(&entry.text, 220),
                self.debug_preview_for_log(entry.raw_text.as_deref().unwrap_or(""), 220)
            ),
        );
        const MAX_LOGS: usize = 200;
        let mut logs = self.transcript_logs.lock();
        logs.push(entry.clone());
        if logs.len() > MAX_LOGS {
            let overflow = logs.len() - MAX_LOGS;
            logs.drain(0..overflow);
        }
        drop(logs);

        // Record aggregated usage stats for final, non-error entries
        if entry.is_final && !entry.text.starts_with("[error]") {
            let word_count = entry.text.split_whitespace().count() as u32;
            self.usage_stats
                .record_dictation(word_count, entry.recording_duration_ms);
        }

        let _ = self.app_handle.emit("transcript-log-added", entry);
    }

    fn push_debug_log(&self, scope: &str, message: String, trace_id: Option<String>) {
        const MAX_LOGS: usize = 1000;
        let entry = DebugLogEntry {
            instance_id: self.instance_id.clone(),
            trace_id,
            timestamp_ms: current_timestamp_ms(),
            scope: scope.to_string(),
            message,
        };

        let mut logs = self.debug_logs.lock();
        logs.push(entry.clone());
        if logs.len() > MAX_LOGS {
            let overflow = logs.len() - MAX_LOGS;
            logs.drain(0..overflow);
        }
        drop(logs);
        let _ = self.app_handle.emit("debug-log-added", entry);
    }

    fn push_error_log(&self, request_id: String, model_id: String, message: String) {
        self.push_transcript_log(TranscriptLogEntry {
            instance_id: self.instance_id.clone(),
            trace_id: request_id.clone(),
            request_id,
            timestamp_ms: current_timestamp_ms(),
            model_id,
            backend: None,
            duration_ms: 0,
            recording_duration_ms: 0,
            transcription_duration_ms: 0,
            cleanup_roundtrip_duration_ms: 0,
            total_waterfall_duration_ms: 0,
            text: format!("[error] {message}"),
            raw_text: None,
            post_process_text: None,
            is_final: true,
            cleanup_requested: false,
            cleanup_applied: false,
            post_process_duration_ms: 0,
            post_process_edits_applied: 0,
            post_process_edits_rejected: 0,
            recording_file: None,
            classification_result: None,
            classification_duration_ms: 0,
            persona_id: None,
            persona_text: None,
            persona_duration_ms: 0,
        });
    }

    fn apply_tray_icon_state(&self, _state: DictationState) {
        // Keep a single static tray icon. macOS microphone indicator is the
        // primary recording state signal.
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn capture_tuning(settings: &Settings) -> CaptureTuning {
    CaptureTuning {
        audio_channel_mode: settings.audio_channel_mode,
        input_gain_db: settings.input_gain_db,
        high_pass_filter: settings.high_pass_filter,
    }
}

/// Returns `true` when a space should be inserted between the end of the
/// previous injection and the start of the new `text`.  A separator is needed
/// when the previous injection ended with a non-whitespace character and the
/// new text begins with an alphanumeric character (or an opening quote/paren)
/// that would otherwise be glued to the prior word.
fn needs_inter_injection_space(prev_trailing: Option<char>, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    match prev_trailing {
        Some(prev) if !prev.is_whitespace() => {
            text.starts_with(|ch: char| ch.is_alphanumeric() || ch == '"' || ch == '(')
        }
        _ => false,
    }
}

const INTER_INJECTION_SPACE_MAX_AGE_MS: u64 = 3_000;

fn recent_inter_injection_context(last_injected_at_ms: Option<u64>, now_ms: u64) -> bool {
    let Some(last) = last_injected_at_ms else {
        return false;
    };
    if now_ms < last {
        return false;
    }
    now_ms.saturating_sub(last) <= INTER_INJECTION_SPACE_MAX_AGE_MS
}

fn should_prepend_inter_injection_space(
    prev_trailing: Option<char>,
    last_injected_at_ms: Option<u64>,
    now_ms: u64,
    text: &str,
) -> bool {
    recent_inter_injection_context(last_injected_at_ms, now_ms)
        && needs_inter_injection_space(prev_trailing, text)
}

fn normalize_transcript_text(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn debug_preview(text: &str, max_chars: usize) -> String {
    let escaped = text
        .trim()
        .chars()
        .flat_map(char::escape_default)
        .collect::<String>();
    if escaped.chars().count() <= max_chars {
        escaped
    } else {
        let preview = escaped.chars().take(max_chars).collect::<String>();
        format!("{preview}…")
    }
}

fn hash_text(text: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    format!("{:x}", digest)[..12].to_string()
}

fn prune_recordings_older_than(
    recordings_dir: &std::path::Path,
    retention: Duration,
) -> Result<()> {
    let now = SystemTime::now();
    let entries = match std::fs::read_dir(recordings_dir) {
        Ok(v) => v,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(anyhow::anyhow!(err)).context("failed reading recordings directory")
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let modified = match metadata.modified() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let age = match now.duration_since(modified) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if age > retention {
            let _ = std::fs::remove_file(path);
        }
    }

    Ok(())
}

fn purge_recordings(recordings_dir: &std::path::Path) -> Result<()> {
    let entries = match std::fs::read_dir(recordings_dir) {
        Ok(v) => v,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(anyhow::anyhow!(err)).context("failed reading recordings directory")
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_file() {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        needs_inter_injection_space, normalize_transcript_text, recent_inter_injection_context,
        should_prepend_inter_injection_space,
    };

    #[test]
    fn normalize_transcript_text_collapses_whitespace() {
        let normalized = normalize_transcript_text("hello   world\t from   buttervoice");
        assert_eq!(normalized, "hello world from buttervoice");
    }

    #[test]
    fn normalize_transcript_text_collapses_line_breaks() {
        let normalized = normalize_transcript_text("first segment\n\nsecond segment");
        assert_eq!(normalized, "first segment second segment");
    }

    #[test]
    fn inter_injection_space_consecutive_words() {
        // "Hello" then "world" → needs space
        assert!(needs_inter_injection_space(Some('o'), "world"));
    }

    #[test]
    fn inter_injection_space_punctuation_attaches() {
        // "Hello" then ", world" → no space (comma attaches)
        assert!(!needs_inter_injection_space(Some('o'), ", world"));
        // "Hello" then "." → no space
        assert!(!needs_inter_injection_space(Some('o'), "."));
        // "Hello" then "!" → no space
        assert!(!needs_inter_injection_space(Some('o'), "!"));
    }

    #[test]
    fn inter_injection_space_first_injection() {
        // No previous injection → no space
        assert!(!needs_inter_injection_space(None, "Hello"));
    }

    #[test]
    fn inter_injection_space_prev_ended_with_whitespace() {
        // Previous ended with newline → no space
        assert!(!needs_inter_injection_space(Some('\n'), "Next line"));
        // Previous ended with space → no space
        assert!(!needs_inter_injection_space(Some(' '), "word"));
    }

    #[test]
    fn inter_injection_space_empty_text() {
        assert!(!needs_inter_injection_space(Some('o'), ""));
    }

    #[test]
    fn inter_injection_space_opening_quote() {
        // "said" then "\"hello\"" → needs space
        assert!(needs_inter_injection_space(Some('d'), "\"hello\""));
    }

    #[test]
    fn inter_injection_space_opening_paren() {
        // "function" then "(args)" → needs space
        assert!(needs_inter_injection_space(Some('n'), "(args)"));
    }

    #[test]
    fn inter_injection_space_requires_recent_context() {
        assert!(should_prepend_inter_injection_space(
            Some('o'),
            Some(10_000),
            12_000,
            "world"
        ));
        assert!(!should_prepend_inter_injection_space(
            Some('o'),
            Some(10_000),
            14_100,
            "world"
        ));
    }

    #[test]
    fn inter_injection_space_no_recent_context_for_first_injection() {
        assert!(!should_prepend_inter_injection_space(
            None, None, 20_000, "Hello"
        ));
    }

    #[test]
    fn recent_context_rejects_clock_skew_backwards() {
        assert!(!recent_inter_injection_context(Some(20_000), 19_999));
    }
}
