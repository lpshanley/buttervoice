// ── Enums (as union types matching Rust serde representations) ──

export type DictationState = 'idle' | 'recording' | 'transcribing' | 'post_processing' | 'injecting' | 'error';

export type PermissionState = 'granted' | 'denied' | 'unknown';
export type PermissionKind = 'microphone' | 'accessibility' | 'input_monitoring';

export type ComputeMode = 'auto' | 'cpu' | 'gpu';
export type SpeechProvider = 'local_whispercpp' | 'remote_openai_compatible';
export type SpeechRemotePreset = 'speaches' | 'openai' | 'custom';
export type AudioChannelMode = 'left' | 'right' | 'mono_mix';
export type HighPassFilter = 'off' | 'hz80' | 'hz120';

export type DictationMode = 'push_to_talk' | 'toggle';
export type OutputDestination = 'input' | 'clipboard' | 'none';

export type HotkeyPreset =
  | 'right_option'
  | 'left_option'
  | 'right_command'
  | 'right_control'
  | 'left_control'
  | 'fn';

export type HotkeyKey = HotkeyPreset | { custom: { keycode: number; is_modifier: boolean } };

export interface HotkeyPresetInfo {
  key: HotkeyKey;
  label: string;
  description: string;
}

export const HOTKEY_LABELS: Record<HotkeyPreset, string> = {
  right_option: 'Right Option (\u2325)',
  left_option: 'Left Option (\u2325)',
  right_command: 'Right Command (\u2318)',
  right_control: 'Right Control (\u2303)',
  left_control: 'Left Control (\u2303)',
  fn: 'Fn (Globe)',
};

export function hotkeyDisplayLabel(key: HotkeyKey): string {
  if (typeof key === 'string') {
    return HOTKEY_LABELS[key] ?? key;
  }
  return `Custom (keycode ${key.custom.keycode})`;
}

// ── Backend data interfaces ──

export interface Persona {
  id: string;
  name: string;
  system_prompt: string;
  model_override: string;
  is_default: boolean;
}

export interface PermissionsStatus {
  microphone: PermissionState;
  accessibility: PermissionState;
  input_monitoring: PermissionState;
}

export interface Settings {
  schema_version: number;
  hotkey: HotkeyKey;
  dictation_mode: DictationMode;
  speech_provider: SpeechProvider;
  speech_remote_preset: SpeechRemotePreset;
  speech_remote_base_url: string;
  speech_remote_model: string;
  speech_remote_api_key: string;
  speech_remote_api_key_configured: boolean;
  model_id: string;
  compute_mode: ComputeMode;
  keep_mic_stream_open: boolean;
  mic_device_id: string | null;
  audio_channel_mode: AudioChannelMode;
  input_gain_db: number;
  high_pass_filter: HighPassFilter;
  launch_at_login: boolean;
  debug_logging: boolean;
  output_destination: OutputDestination;
  llm_cleanup_enabled: boolean;
  llm_cleanup_model: string;
  llm_cleanup_api_key: string;
  llm_cleanup_api_key_configured: boolean;
  llm_cleanup_base_url: string;
  llm_cleanup_use_custom_prompt: boolean;
  llm_cleanup_custom_prompt: string;
  llm_cleanup_model_override: string;

  // Content classification
  content_classification_enabled: boolean;
  content_classification_auto_apply: boolean;
  content_classification_model_override: string;
  content_classification_use_custom_prompt: boolean;
  content_classification_custom_prompt: string;
  content_classification_warning_threshold: number;
  content_classification_block_threshold: number;

  // Personas
  persona_enabled: boolean;
  persona_auto_apply: boolean;
  persona_active_id: string;
  personas: Persona[];
  beta_ai_enhancement_enabled: boolean;
  beta_content_classification_enabled: boolean;
  beta_personas_enabled: boolean;

  debug_log_include_content: boolean;
  recording_retention_hours: number;
  debug_log_retention_hours: number;
  whisper_language: string;
  whisper_beam_size: number;
  whisper_prompt: string;
  whisper_no_speech_thold: number;
  whisper_temperature: number;
  whisper_temperature_inc: number;
  post_process_enabled: boolean;
  post_process_spell_enabled: boolean;
  post_process_itn_enabled: boolean;
  post_process_grammar_rules_enabled: boolean;
  post_process_gec_enabled: boolean;
  post_process_custom_dictionary: string[];
  post_process_confidence_threshold: number;
  post_process_max_edit_ratio: number;
}

export interface SettingsPatch {
  hotkey?: HotkeyKey;
  dictation_mode?: DictationMode;
  speech_provider?: SpeechProvider;
  speech_remote_preset?: SpeechRemotePreset;
  speech_remote_base_url?: string;
  speech_remote_model?: string;
  speech_remote_api_key?: string;
  model_id?: string;
  compute_mode?: ComputeMode;
  keep_mic_stream_open?: boolean;
  mic_device_id?: string | null;
  audio_channel_mode?: AudioChannelMode;
  input_gain_db?: number;
  high_pass_filter?: HighPassFilter;
  launch_at_login?: boolean;
  debug_logging?: boolean;
  output_destination?: OutputDestination;
  llm_cleanup_enabled?: boolean;
  llm_cleanup_model?: string;
  llm_cleanup_api_key?: string;
  llm_cleanup_base_url?: string;
  llm_cleanup_use_custom_prompt?: boolean;
  llm_cleanup_custom_prompt?: string;
  llm_cleanup_model_override?: string;

  // Content classification
  content_classification_enabled?: boolean;
  content_classification_auto_apply?: boolean;
  content_classification_model_override?: string;
  content_classification_use_custom_prompt?: boolean;
  content_classification_custom_prompt?: string;
  content_classification_warning_threshold?: number;
  content_classification_block_threshold?: number;

  // Personas
  persona_enabled?: boolean;
  persona_auto_apply?: boolean;
  persona_active_id?: string;
  personas?: Persona[];
  beta_ai_enhancement_enabled?: boolean;
  beta_content_classification_enabled?: boolean;
  beta_personas_enabled?: boolean;

  debug_log_include_content?: boolean;
  recording_retention_hours?: number;
  debug_log_retention_hours?: number;
  whisper_language?: string;
  whisper_beam_size?: number;
  whisper_prompt?: string;
  whisper_no_speech_thold?: number;
  whisper_temperature?: number;
  whisper_temperature_inc?: number;
  post_process_enabled?: boolean;
  post_process_spell_enabled?: boolean;
  post_process_itn_enabled?: boolean;
  post_process_grammar_rules_enabled?: boolean;
  post_process_gec_enabled?: boolean;
  post_process_custom_dictionary?: string[];
  post_process_confidence_threshold?: number;
  post_process_max_edit_ratio?: number;
}

export interface MicDevice {
  id: string;
  name: string;
}

export interface ModelInfo {
  id: string;
  display_name: string;
  family: string;
  family_order: number;
  estimated_size_mb: number;
  recommended: boolean;
  quantized: boolean;
}

export interface LlmModelEntry {
  id: string;
  name: string | null;
}

export interface SpeechModelEntry {
  id: string;
  name: string | null;
}

export interface Persona {
  id: string;
  name: string;
  system_prompt: string;
  model_override: string;
  is_default: boolean;
}

export interface ClassificationTag {
  tag: string;
  score: number;
  severity: string;
}

export interface ContentClassificationResult {
  score: number;
  categories: ClassificationTag[];
  blocked: boolean;
  warning: boolean;
}

export interface TranscriptLogEntry {
  instance_id: string;
  trace_id: string;
  request_id: string;
  timestamp_ms: number;
  model_id: string;
  backend: string | null;
  duration_ms: number;
  recording_duration_ms: number;
  transcription_duration_ms: number;
  cleanup_roundtrip_duration_ms: number;
  total_waterfall_duration_ms: number;
  text: string;
  raw_text: string | null;
  post_process_text: string | null;
  is_final: boolean;
  cleanup_requested: boolean;
  cleanup_applied: boolean;
  post_process_duration_ms: number;
  post_process_edits_applied: number;
  post_process_edits_rejected: number;
  recording_file: string | null;
  classification_result: ContentClassificationResult | null;
  classification_duration_ms: number;
  persona_id: string | null;
  persona_text: string | null;
  persona_duration_ms: number;
}

export interface DebugLogEntry {
  instance_id: string;
  trace_id: string | null;
  timestamp_ms: number;
  scope: string;
  message: string;
}

export interface ModelDownloadProgress {
  model_id: string;
  downloaded_bytes: number;
  total_bytes: number;
  status: 'downloading' | 'completed' | 'cancelled' | 'error';
  error: string | null;
}

export interface BackendStatus {
  ok: boolean;
  backend: string;
  active_provider: SpeechProvider;
  provider_label: string;
  provider_ok: boolean;
  provider_error: string | null;
  remote_base_url: string | null;
  remote_model: string | null;
  binary_available: boolean;
  binary_path: string | null;
  selected_compute_mode: string;
  effective_compute_mode: string | null;
  last_fallback_reason: string | null;
}

// ── Post-processing types ──

export type PipelineStage =
  | 'sentence_segmentation'
  | 'punctuation'
  | 'truecasing'
  | 'inverse_text_norm'
  | 'spell_correction'
  | 'grammar_rules'
  | 'grammar_gec';

export interface TextEdit {
  offset: number;
  length: number;
  replacement: string;
  source: PipelineStage;
  confidence: number;
  rule_id: string;
}

export interface PipelineResult {
  input: string;
  output: string;
  applied_edits: TextEdit[];
  rejected_edits: TextEdit[];
  stage_timings_ms: Record<string, number>;
  total_duration_ms: number;
}

export interface StageLatencyHistogram {
  le_50_ms: number;
  le_100_ms: number;
  le_250_ms: number;
  le_500_ms: number;
  le_1000_ms: number;
  gt_1000_ms: number;
}

export interface PipelineFailureEvent {
  timestamp_ms: number;
  stage: string;
  error_code: string;
}

export interface PipelineMetricsSnapshot {
  dictations_started: number;
  dictations_succeeded: number;
  dictations_failed: number;
  pp_runs: number;
  pp_edits_applied_total: number;
  pp_edits_rejected_total: number;
  llm_attempts: number;
  llm_success: number;
  llm_fail: number;
  llm_timeout: number;
  llm_skipped_circuit_open: number;
  stage_latency_histograms: Record<string, StageLatencyHistogram>;
  last_100_failures: PipelineFailureEvent[];
  llm_circuit_open: boolean;
  llm_circuit_open_until_ms: number | null;
}

// ── Usage stats ──

export interface DailyStat {
  date: string;
  word_count: number;
  dictation_count: number;
  recording_seconds: number;
}

// ── UI types ──

export type ToastKind = 'info' | 'success' | 'error';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  exiting: boolean;
}

export type RoundtripSegmentKey = 'transcription' | 'post_process' | 'enhancement' | 'classification' | 'persona';

export interface RoundtripSegment {
  key: RoundtripSegmentKey;
  label: string;
  ms: number;
  share: number;
}
