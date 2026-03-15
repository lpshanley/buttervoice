use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::de::Error as DeError;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::hotkey_macos::{DictationMode, HotkeyKey};
use crate::models;
use crate::secrets;

pub const SETTINGS_SCHEMA_VERSION: u32 = 14;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioChannelMode {
    #[default]
    Left,
    Right,
    MonoMix,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HighPassFilter {
    Off,
    #[default]
    Hz80,
    Hz120,
}

impl HighPassFilter {
    pub fn cutoff_hz(self) -> Option<f32> {
        match self {
            Self::Off => None,
            Self::Hz80 => Some(80.0),
            Self::Hz120 => Some(120.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputDestination {
    /// Type text directly into the focused input field.
    #[default]
    Input,
    /// Copy text to the system clipboard.
    Clipboard,
    /// Do nothing — keep text in ButterVoice only.
    None,
}

impl OutputDestination {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Input => 0,
            Self::Clipboard => 1,
            Self::None => 2,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Input,
            1 => Self::Clipboard,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComputeMode {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

impl ComputeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeechProvider {
    #[default]
    LocalWhispercpp,
    RemoteOpenaiCompatible,
}

impl SpeechProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalWhispercpp => "local_whispercpp",
            Self::RemoteOpenaiCompatible => "remote_openai_compatible",
        }
    }

    pub fn is_local(self) -> bool {
        matches!(self, Self::LocalWhispercpp)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeechRemotePreset {
    #[default]
    Speaches,
    Openai,
    Custom,
}

impl SpeechRemotePreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Speaches => "speaches",
            Self::Openai => "openai",
            Self::Custom => "custom",
        }
    }
}

// ── Persona ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub model_override: String,
    pub is_default: bool,
}

pub const DEFAULT_PERSONA_PROFESSIONAL_ID: &str = "builtin-professional-tone";
pub const DEFAULT_PERSONA_PROMPT_ENGINEER_ID: &str = "builtin-prompt-engineer";

pub fn default_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: DEFAULT_PERSONA_PROFESSIONAL_ID.to_string(),
            name: "Professional Tone".to_string(),
            system_prompt: crate::persona::DEFAULT_PROFESSIONAL_TONE_PROMPT.to_string(),
            model_override: String::new(),
            is_default: true,
        },
        Persona {
            id: DEFAULT_PERSONA_PROMPT_ENGINEER_ID.to_string(),
            name: "Prompt Engineer".to_string(),
            system_prompt: crate::persona::DEFAULT_PROMPT_ENGINEER_PROMPT.to_string(),
            model_override: String::new(),
            is_default: true,
        },
    ]
}

// ── Settings ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub hotkey: HotkeyKey,
    pub dictation_mode: DictationMode,
    pub speech_provider: SpeechProvider,
    pub speech_remote_preset: SpeechRemotePreset,
    pub speech_remote_base_url: String,
    pub speech_remote_model: String,
    #[serde(default, skip_serializing)]
    pub speech_remote_api_key: String,
    pub speech_remote_api_key_configured: bool,
    pub model_id: String,
    pub compute_mode: ComputeMode,
    pub keep_mic_stream_open: bool,
    pub mic_device_id: Option<String>,
    pub audio_channel_mode: AudioChannelMode,
    pub input_gain_db: f32,
    pub high_pass_filter: HighPassFilter,
    pub launch_at_login: bool,
    pub debug_logging: bool,
    pub output_destination: OutputDestination,
    pub llm_cleanup_enabled: bool,
    pub llm_cleanup_model: String,
    #[serde(default, skip_serializing)]
    pub llm_cleanup_api_key: String,
    pub llm_cleanup_api_key_configured: bool,
    pub llm_cleanup_base_url: String,
    pub llm_cleanup_use_custom_prompt: bool,
    pub llm_cleanup_custom_prompt: String,
    pub llm_cleanup_model_override: String,

    // Content classification
    pub content_classification_enabled: bool,
    pub content_classification_auto_apply: bool,
    pub content_classification_model_override: String,
    pub content_classification_use_custom_prompt: bool,
    pub content_classification_custom_prompt: String,
    pub content_classification_warning_threshold: f32,
    pub content_classification_block_threshold: f32,

    // Personas
    pub persona_enabled: bool,
    pub persona_auto_apply: bool,
    pub persona_active_id: String,
    pub personas: Vec<Persona>,
    pub beta_ai_enhancement_enabled: bool,
    pub beta_content_classification_enabled: bool,
    pub beta_personas_enabled: bool,

    pub debug_log_include_content: bool,
    pub recording_retention_hours: u16,
    pub debug_log_retention_hours: u16,
    pub whisper_language: String,
    pub whisper_beam_size: u32,
    pub whisper_prompt: String,
    pub whisper_no_speech_thold: f32,
    pub whisper_temperature: f32,
    pub whisper_temperature_inc: f32,
    pub whisper_threads: u32,

    // Post-processing pipeline
    pub post_process_enabled: bool,
    pub post_process_spell_enabled: bool,
    pub post_process_itn_enabled: bool,
    pub post_process_grammar_rules_enabled: bool,
    pub post_process_gec_enabled: bool,
    pub post_process_custom_dictionary: Vec<String>,
    pub post_process_confidence_threshold: f32,
    pub post_process_max_edit_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsPatch {
    pub hotkey: Option<HotkeyKey>,
    pub dictation_mode: Option<DictationMode>,
    pub speech_provider: Option<SpeechProvider>,
    pub speech_remote_preset: Option<SpeechRemotePreset>,
    pub speech_remote_base_url: Option<String>,
    pub speech_remote_model: Option<String>,
    pub speech_remote_api_key: Option<String>,
    pub model_id: Option<String>,
    pub compute_mode: Option<ComputeMode>,
    pub keep_mic_stream_open: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_patch_mic_device_id")]
    pub mic_device_id: Option<Option<String>>,
    pub audio_channel_mode: Option<AudioChannelMode>,
    pub input_gain_db: Option<f32>,
    pub high_pass_filter: Option<HighPassFilter>,
    pub launch_at_login: Option<bool>,
    pub debug_logging: Option<bool>,
    pub output_destination: Option<OutputDestination>,
    pub llm_cleanup_enabled: Option<bool>,
    pub llm_cleanup_model: Option<String>,
    pub llm_cleanup_api_key: Option<String>,
    pub llm_cleanup_base_url: Option<String>,
    pub llm_cleanup_use_custom_prompt: Option<bool>,
    pub llm_cleanup_custom_prompt: Option<String>,
    pub llm_cleanup_model_override: Option<String>,

    // Content classification
    pub content_classification_enabled: Option<bool>,
    pub content_classification_auto_apply: Option<bool>,
    pub content_classification_model_override: Option<String>,
    pub content_classification_use_custom_prompt: Option<bool>,
    pub content_classification_custom_prompt: Option<String>,
    pub content_classification_warning_threshold: Option<f32>,
    pub content_classification_block_threshold: Option<f32>,

    // Personas
    pub persona_enabled: Option<bool>,
    pub persona_auto_apply: Option<bool>,
    pub persona_active_id: Option<String>,
    pub personas: Option<Vec<Persona>>,
    pub beta_ai_enhancement_enabled: Option<bool>,
    pub beta_content_classification_enabled: Option<bool>,
    pub beta_personas_enabled: Option<bool>,

    pub debug_log_include_content: Option<bool>,
    pub recording_retention_hours: Option<u16>,
    pub debug_log_retention_hours: Option<u16>,
    pub whisper_language: Option<String>,
    pub whisper_beam_size: Option<u32>,
    pub whisper_prompt: Option<String>,
    pub whisper_no_speech_thold: Option<f32>,
    pub whisper_temperature: Option<f32>,
    pub whisper_temperature_inc: Option<f32>,
    pub whisper_threads: Option<u32>,

    // Post-processing pipeline
    pub post_process_enabled: Option<bool>,
    pub post_process_spell_enabled: Option<bool>,
    pub post_process_itn_enabled: Option<bool>,
    pub post_process_grammar_rules_enabled: Option<bool>,
    pub post_process_gec_enabled: Option<bool>,
    pub post_process_custom_dictionary: Option<Vec<String>>,
    pub post_process_confidence_threshold: Option<f32>,
    pub post_process_max_edit_ratio: Option<f32>,
}

fn deserialize_patch_mic_device_id<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(Some(None)),
        serde_json::Value::String(mic_id) => Ok(Some(Some(mic_id))),
        other => Err(D::Error::custom(format!(
            "expected string or null for mic_device_id, got {other}"
        ))),
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            hotkey: HotkeyKey::default(),
            dictation_mode: DictationMode::default(),
            speech_provider: SpeechProvider::default(),
            speech_remote_preset: SpeechRemotePreset::default(),
            speech_remote_base_url: String::new(),
            speech_remote_model: String::new(),
            speech_remote_api_key: String::new(),
            speech_remote_api_key_configured: false,
            model_id: models::default_model_id().to_string(),
            compute_mode: ComputeMode::default(),
            keep_mic_stream_open: false,
            mic_device_id: None,
            audio_channel_mode: AudioChannelMode::default(),
            input_gain_db: 0.0,
            high_pass_filter: HighPassFilter::default(),
            launch_at_login: false,
            debug_logging: false,
            output_destination: OutputDestination::Input,
            llm_cleanup_enabled: false,
            llm_cleanup_model: "openai/gpt-4o-mini".to_string(),
            llm_cleanup_api_key: String::new(),
            llm_cleanup_api_key_configured: false,
            llm_cleanup_base_url: "https://openrouter.ai/api/v1".to_string(),
            llm_cleanup_use_custom_prompt: false,
            llm_cleanup_custom_prompt: String::new(),
            llm_cleanup_model_override: String::new(),
            content_classification_enabled: false,
            content_classification_auto_apply: false,
            content_classification_model_override: String::new(),
            content_classification_use_custom_prompt: false,
            content_classification_custom_prompt: String::new(),
            content_classification_warning_threshold: 0.3,
            content_classification_block_threshold: 0.7,
            persona_enabled: false,
            persona_auto_apply: false,
            persona_active_id: DEFAULT_PERSONA_PROFESSIONAL_ID.to_string(),
            personas: default_personas(),
            beta_ai_enhancement_enabled: false,
            beta_content_classification_enabled: false,
            beta_personas_enabled: false,
            debug_log_include_content: false,
            recording_retention_hours: 24,
            debug_log_retention_hours: 24,
            whisper_language: "en".to_string(),
            whisper_beam_size: 2,
            whisper_prompt: "Hello. Use proper punctuation, capitalization, and formatting."
                .to_string(),
            whisper_no_speech_thold: 0.6,
            whisper_temperature: 0.0,
            whisper_temperature_inc: 0.2,
            whisper_threads: 0,
            post_process_enabled: false,
            post_process_spell_enabled: true,
            post_process_itn_enabled: true,
            post_process_grammar_rules_enabled: true,
            post_process_gec_enabled: false,
            post_process_custom_dictionary: Vec::new(),
            post_process_confidence_threshold: 0.7,
            post_process_max_edit_ratio: 0.3,
        }
    }
}

impl Settings {
    pub fn active_speech_model_id(&self) -> String {
        match self.speech_provider {
            SpeechProvider::LocalWhispercpp => self.model_id.clone(),
            SpeechProvider::RemoteOpenaiCompatible => self.speech_remote_model.trim().to_string(),
        }
    }
}

#[derive(Debug)]
pub struct SettingsStore {
    app_handle: AppHandle,
    cache: RwLock<Settings>,
}

impl SettingsStore {
    pub fn new(app_handle: AppHandle) -> Result<Self> {
        let store = app_handle
            .store("settings.json")
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut settings = store
            .get("config")
            .and_then(|v| serde_json::from_value::<Settings>(v).ok())
            .unwrap_or_else(|| {
                let defaults = Settings::default();
                store.set(
                    "config",
                    serde_json::to_value(&defaults).unwrap_or_default(),
                );
                let _ = store.save();
                defaults
            });

        let mut requires_save = false;

        if !settings.speech_remote_api_key.trim().is_empty() {
            match secrets::store_speech_api_key(&settings.speech_remote_api_key) {
                Ok(configured) => {
                    settings.speech_remote_api_key_configured = configured;
                }
                Err(err) => {
                    eprintln!("failed migrating speech API key to Keychain: {err:#}");
                }
            }
            settings.speech_remote_api_key.clear();
            requires_save = true;
        } else if let Ok(configured) = secrets::load_speech_api_key().map(|v| v.is_some()) {
            settings.speech_remote_api_key_configured = configured;
            requires_save = true;
        }

        if !settings.llm_cleanup_api_key.trim().is_empty() {
            match secrets::store_llm_api_key(&settings.llm_cleanup_api_key) {
                Ok(configured) => {
                    settings.llm_cleanup_api_key_configured = configured;
                }
                Err(err) => {
                    eprintln!("failed migrating API key to Keychain: {err:#}");
                }
            }
            settings.llm_cleanup_api_key.clear();
            requires_save = true;
        } else if let Ok(configured) = secrets::load_llm_api_key().map(|v| v.is_some()) {
            settings.llm_cleanup_api_key_configured = configured;
            requires_save = true;
        }

        // Ensure built-in personas exist after migration from older schemas.
        if settings.personas.is_empty() {
            settings.personas = default_personas();
            requires_save = true;
        } else {
            // Ensure both built-in personas are present (may be missing after partial migration).
            let has_professional = settings
                .personas
                .iter()
                .any(|p| p.id == DEFAULT_PERSONA_PROFESSIONAL_ID);
            let has_prompt_eng = settings
                .personas
                .iter()
                .any(|p| p.id == DEFAULT_PERSONA_PROMPT_ENGINEER_ID);
            if !has_professional || !has_prompt_eng {
                for default_p in default_personas() {
                    if !settings.personas.iter().any(|p| p.id == default_p.id) {
                        settings.personas.push(default_p);
                    }
                }
                requires_save = true;
            }
        }

        if settings.schema_version != SETTINGS_SCHEMA_VERSION {
            settings.schema_version = SETTINGS_SCHEMA_VERSION;
            requires_save = true;
        }

        if requires_save {
            store.set(
                "config",
                serde_json::to_value(&settings)
                    .context("failed serializing settings for migration")?,
            );
            let _ = store.save();
        }

        Ok(Self {
            app_handle,
            cache: RwLock::new(settings),
        })
    }

    pub fn get(&self) -> Settings {
        self.cache.read().clone()
    }

    pub fn update(&self, patch: SettingsPatch) -> Result<Settings> {
        let mut settings = self.cache.write();

        if let Some(hotkey) = patch.hotkey {
            settings.hotkey = hotkey;
        }
        if let Some(dictation_mode) = patch.dictation_mode {
            settings.dictation_mode = dictation_mode;
        }
        if let Some(speech_provider) = patch.speech_provider {
            settings.speech_provider = speech_provider;
        }
        if let Some(speech_remote_preset) = patch.speech_remote_preset {
            settings.speech_remote_preset = speech_remote_preset;
        }
        if let Some(speech_remote_base_url) = patch.speech_remote_base_url {
            settings.speech_remote_base_url = normalize_base_url(&speech_remote_base_url);
        }
        if let Some(speech_remote_model) = patch.speech_remote_model {
            settings.speech_remote_model = speech_remote_model.trim().to_string();
        }
        if let Some(speech_remote_api_key) = patch.speech_remote_api_key {
            let configured = secrets::store_speech_api_key(&speech_remote_api_key)
                .context("failed storing speech_remote_api_key in Keychain")?;
            settings.speech_remote_api_key.clear();
            settings.speech_remote_api_key_configured = configured;
        }
        if let Some(model_id) = patch.model_id {
            settings.model_id = model_id;
        }
        if let Some(compute_mode) = patch.compute_mode {
            settings.compute_mode = compute_mode;
        }
        if let Some(keep_mic_stream_open) = patch.keep_mic_stream_open {
            settings.keep_mic_stream_open = keep_mic_stream_open;
        }
        if let Some(mic_device_id) = patch.mic_device_id {
            settings.mic_device_id = mic_device_id;
        }
        if let Some(audio_channel_mode) = patch.audio_channel_mode {
            settings.audio_channel_mode = audio_channel_mode;
        }
        if let Some(input_gain_db) = patch.input_gain_db {
            settings.input_gain_db = input_gain_db.clamp(-24.0, 24.0);
        }
        if let Some(high_pass_filter) = patch.high_pass_filter {
            settings.high_pass_filter = high_pass_filter;
        }
        if let Some(launch_at_login) = patch.launch_at_login {
            settings.launch_at_login = launch_at_login;
        }
        if let Some(debug_logging) = patch.debug_logging {
            settings.debug_logging = debug_logging;
        }
        if let Some(output_destination) = patch.output_destination {
            settings.output_destination = output_destination;
        }
        if let Some(llm_cleanup_enabled) = patch.llm_cleanup_enabled {
            settings.llm_cleanup_enabled = llm_cleanup_enabled;
        }
        if let Some(llm_cleanup_model) = patch.llm_cleanup_model {
            settings.llm_cleanup_model = llm_cleanup_model.trim().to_string();
        }
        if let Some(llm_cleanup_api_key) = patch.llm_cleanup_api_key {
            let configured = secrets::store_llm_api_key(&llm_cleanup_api_key)
                .context("failed storing llm_cleanup_api_key in Keychain")?;
            settings.llm_cleanup_api_key.clear();
            settings.llm_cleanup_api_key_configured = configured;
        }
        if let Some(llm_cleanup_base_url) = patch.llm_cleanup_base_url {
            settings.llm_cleanup_base_url = normalize_base_url(&llm_cleanup_base_url);
        }
        if let Some(llm_cleanup_use_custom_prompt) = patch.llm_cleanup_use_custom_prompt {
            settings.llm_cleanup_use_custom_prompt = llm_cleanup_use_custom_prompt;
        }
        if let Some(llm_cleanup_custom_prompt) = patch.llm_cleanup_custom_prompt {
            settings.llm_cleanup_custom_prompt = llm_cleanup_custom_prompt;
        }
        if let Some(llm_cleanup_model_override) = patch.llm_cleanup_model_override {
            settings.llm_cleanup_model_override = llm_cleanup_model_override.trim().to_string();
        }
        if let Some(v) = patch.content_classification_enabled {
            settings.content_classification_enabled = v;
        }
        if let Some(v) = patch.content_classification_auto_apply {
            settings.content_classification_auto_apply = v;
        }
        if let Some(v) = patch.content_classification_model_override {
            settings.content_classification_model_override = v.trim().to_string();
        }
        if let Some(v) = patch.content_classification_use_custom_prompt {
            settings.content_classification_use_custom_prompt = v;
        }
        if let Some(v) = patch.content_classification_custom_prompt {
            settings.content_classification_custom_prompt = v;
        }
        if let Some(v) = patch.content_classification_warning_threshold {
            settings.content_classification_warning_threshold = v.clamp(0.0, 1.0);
        }
        if let Some(v) = patch.content_classification_block_threshold {
            settings.content_classification_block_threshold = v.clamp(0.0, 1.0);
        }
        if let Some(v) = patch.persona_enabled {
            settings.persona_enabled = v;
        }
        if let Some(v) = patch.persona_auto_apply {
            settings.persona_auto_apply = v;
        }
        if let Some(v) = patch.persona_active_id {
            settings.persona_active_id = v;
        }
        if let Some(v) = patch.personas {
            settings.personas = v;
        }
        if let Some(v) = patch.beta_ai_enhancement_enabled {
            settings.beta_ai_enhancement_enabled = v;
        }
        if let Some(v) = patch.beta_content_classification_enabled {
            settings.beta_content_classification_enabled = v;
        }
        if let Some(v) = patch.beta_personas_enabled {
            settings.beta_personas_enabled = v;
        }
        if let Some(debug_log_include_content) = patch.debug_log_include_content {
            settings.debug_log_include_content = debug_log_include_content;
        }
        if let Some(recording_retention_hours) = patch.recording_retention_hours {
            settings.recording_retention_hours = recording_retention_hours.clamp(1, 24 * 30);
        }
        if let Some(debug_log_retention_hours) = patch.debug_log_retention_hours {
            settings.debug_log_retention_hours = debug_log_retention_hours.clamp(1, 24 * 30);
        }
        if let Some(whisper_language) = patch.whisper_language {
            settings.whisper_language = whisper_language.trim().to_string();
        }
        if let Some(whisper_beam_size) = patch.whisper_beam_size {
            settings.whisper_beam_size = whisper_beam_size.clamp(1, 10);
        }
        if let Some(whisper_prompt) = patch.whisper_prompt {
            settings.whisper_prompt = whisper_prompt;
        }
        if let Some(whisper_no_speech_thold) = patch.whisper_no_speech_thold {
            settings.whisper_no_speech_thold = whisper_no_speech_thold.clamp(0.0, 1.0);
        }
        if let Some(whisper_temperature) = patch.whisper_temperature {
            settings.whisper_temperature = whisper_temperature.clamp(0.0, 1.0);
        }
        if let Some(whisper_temperature_inc) = patch.whisper_temperature_inc {
            settings.whisper_temperature_inc = whisper_temperature_inc.clamp(0.0, 1.0);
        }
        if let Some(whisper_threads) = patch.whisper_threads {
            settings.whisper_threads = whisper_threads.min(64);
        }
        if let Some(post_process_enabled) = patch.post_process_enabled {
            settings.post_process_enabled = post_process_enabled;
        }
        if let Some(post_process_spell_enabled) = patch.post_process_spell_enabled {
            settings.post_process_spell_enabled = post_process_spell_enabled;
        }
        if let Some(post_process_itn_enabled) = patch.post_process_itn_enabled {
            settings.post_process_itn_enabled = post_process_itn_enabled;
        }
        if let Some(post_process_grammar_rules_enabled) = patch.post_process_grammar_rules_enabled {
            settings.post_process_grammar_rules_enabled = post_process_grammar_rules_enabled;
        }
        if let Some(post_process_gec_enabled) = patch.post_process_gec_enabled {
            settings.post_process_gec_enabled = post_process_gec_enabled;
        }
        if let Some(post_process_custom_dictionary) = patch.post_process_custom_dictionary {
            settings.post_process_custom_dictionary = post_process_custom_dictionary;
        }
        if let Some(post_process_confidence_threshold) = patch.post_process_confidence_threshold {
            settings.post_process_confidence_threshold =
                post_process_confidence_threshold.clamp(0.0, 1.0);
        }
        if let Some(post_process_max_edit_ratio) = patch.post_process_max_edit_ratio {
            settings.post_process_max_edit_ratio = post_process_max_edit_ratio.clamp(0.0, 1.0);
        }

        settings.schema_version = SETTINGS_SCHEMA_VERSION;

        let store = self
            .app_handle
            .store("settings.json")
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        store.set(
            "config",
            serde_json::to_value(&*settings).context("failed serializing settings for store")?,
        );
        store.save().map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(settings.clone())
    }
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

pub fn app_support_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let base = PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("ButterVoice");
    Ok(base)
}

pub fn models_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("models")
}

#[cfg(test)]
mod tests {
    use super::{
        AudioChannelMode, ComputeMode, HighPassFilter, OutputDestination, Settings, SettingsPatch,
        SpeechProvider, SpeechRemotePreset,
    };
    use crate::hotkey_macos::{DictationMode, HotkeyKey};

    #[test]
    fn settings_patch_treats_missing_mic_as_no_change() {
        let patch: SettingsPatch = serde_json::from_str(r#"{"hotkey":"right_option"}"#).unwrap();
        assert_eq!(patch.mic_device_id, None);
        assert_eq!(patch.hotkey, Some(HotkeyKey::RightOption));
    }

    #[test]
    fn settings_patch_treats_null_mic_as_default_device() {
        let patch: SettingsPatch = serde_json::from_str(r#"{"mic_device_id":null}"#).unwrap();
        assert_eq!(patch.mic_device_id, Some(None));
    }

    #[test]
    fn settings_patch_parses_specific_mic_value() {
        let patch: SettingsPatch =
            serde_json::from_str(r#"{"mic_device_id":"Built-in Microphone"}"#).unwrap();
        assert_eq!(
            patch.mic_device_id,
            Some(Some("Built-in Microphone".to_string()))
        );
    }

    #[test]
    fn settings_defaults_compute_mode_during_migration() {
        // v6 JSON with hotkey as string "right_option" — seamless migration
        let settings: Settings = serde_json::from_str(
            r#"{"schema_version":6,"hotkey":"right_option","model_id":"base.en","mic_device_id":null,"launch_at_login":false}"#,
        )
        .unwrap();
        assert_eq!(settings.hotkey, HotkeyKey::RightOption);
        assert_eq!(settings.dictation_mode, DictationMode::PushToTalk);
        assert_eq!(settings.speech_provider, SpeechProvider::LocalWhispercpp);
        assert_eq!(settings.speech_remote_preset, SpeechRemotePreset::Speaches);
        assert!(settings.speech_remote_base_url.is_empty());
        assert!(settings.speech_remote_model.is_empty());
        assert!(settings.speech_remote_api_key.is_empty());
        assert!(!settings.speech_remote_api_key_configured);
        assert!(!settings.beta_ai_enhancement_enabled);
        assert!(!settings.beta_content_classification_enabled);
        assert!(!settings.beta_personas_enabled);
        assert_eq!(settings.compute_mode, ComputeMode::Auto);
        assert!(!settings.keep_mic_stream_open);
        assert_eq!(settings.audio_channel_mode, AudioChannelMode::Left);
        assert_eq!(settings.input_gain_db, 0.0);
        assert_eq!(settings.high_pass_filter, HighPassFilter::Hz80);
        assert!(!settings.debug_logging);
        assert_eq!(settings.output_destination, OutputDestination::Input);
        assert!(!settings.llm_cleanup_enabled);
        assert_eq!(settings.llm_cleanup_model, "openai/gpt-4o-mini");
        assert!(settings.llm_cleanup_api_key.is_empty());
        assert!(!settings.llm_cleanup_api_key_configured);
        assert_eq!(
            settings.llm_cleanup_base_url,
            "https://openrouter.ai/api/v1"
        );
        assert!(!settings.debug_log_include_content);
        assert_eq!(settings.recording_retention_hours, 24);
        assert_eq!(settings.debug_log_retention_hours, 24);
    }

    #[test]
    fn settings_parses_new_hotkey_variants() {
        let patch: SettingsPatch = serde_json::from_str(r#"{"hotkey":"fn"}"#).unwrap();
        assert_eq!(patch.hotkey, Some(HotkeyKey::Fn));

        let patch: SettingsPatch = serde_json::from_str(r#"{"hotkey":"left_option"}"#).unwrap();
        assert_eq!(patch.hotkey, Some(HotkeyKey::LeftOption));

        let patch: SettingsPatch = serde_json::from_str(r#"{"hotkey":"right_command"}"#).unwrap();
        assert_eq!(patch.hotkey, Some(HotkeyKey::RightCommand));
    }

    #[test]
    fn settings_v7_migration_gets_whisper_tuning_defaults() {
        let settings: Settings = serde_json::from_str(
            r#"{"schema_version":7,"hotkey":"right_option","model_id":"base.en"}"#,
        )
        .unwrap();
        assert_eq!(settings.whisper_language, "en");
        assert_eq!(settings.whisper_beam_size, 2);
        assert!(!settings.whisper_prompt.is_empty());
        assert!((settings.whisper_no_speech_thold - 0.6).abs() < f32::EPSILON);
        assert!((settings.whisper_temperature - 0.0).abs() < f32::EPSILON);
        assert!((settings.whisper_temperature_inc - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn settings_v12_migration_gets_speech_provider_defaults() {
        let settings: Settings = serde_json::from_str(
            r#"{"schema_version":12,"hotkey":"right_option","model_id":"base.en"}"#,
        )
        .unwrap();
        assert_eq!(settings.speech_provider, SpeechProvider::LocalWhispercpp);
        assert_eq!(settings.speech_remote_preset, SpeechRemotePreset::Speaches);
        assert!(settings.speech_remote_base_url.is_empty());
        assert!(settings.speech_remote_model.is_empty());
        assert!(settings.speech_remote_api_key.is_empty());
        assert!(!settings.speech_remote_api_key_configured);
        assert!(!settings.beta_ai_enhancement_enabled);
        assert!(!settings.beta_content_classification_enabled);
        assert!(!settings.beta_personas_enabled);
    }

    #[test]
    fn speech_api_key_is_not_serialized() {
        let settings = Settings {
            speech_remote_api_key: "secret".to_string(),
            speech_remote_api_key_configured: true,
            ..Settings::default()
        };

        let raw = serde_json::to_value(settings).unwrap();
        let obj = raw.as_object().unwrap();
        assert!(!obj.contains_key("speech_remote_api_key"));
        assert_eq!(
            obj.get("speech_remote_api_key_configured")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn settings_parses_dictation_mode() {
        let patch: SettingsPatch = serde_json::from_str(r#"{"dictation_mode":"toggle"}"#).unwrap();
        assert_eq!(patch.dictation_mode, Some(DictationMode::Toggle));

        let patch: SettingsPatch =
            serde_json::from_str(r#"{"dictation_mode":"push_to_talk"}"#).unwrap();
        assert_eq!(patch.dictation_mode, Some(DictationMode::PushToTalk));
    }
}
