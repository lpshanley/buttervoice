import { queryOptions } from '@tanstack/react-query';
import { invoke } from './tauri';
import type {
  BackendStatus,
  DailyStat,
  DebugLogEntry,
  DictationState,
  HotkeyPresetInfo,
  LlmModelEntry,
  MicDevice,
  ModelInfo,
  PermissionKind,
  PermissionsStatus,
  PipelineMetricsSnapshot,
  Settings,
  SettingsPatch,
  SpeechModelEntry,
  TranscriptLogEntry,
} from '../types';

// ── Query options (for TanStack Query useQuery) ──

export const settingsQuery = queryOptions({
  queryKey: ['settings'],
  queryFn: () => invoke<Settings>('get_settings'),
});

export const modelsQuery = queryOptions({
  queryKey: ['models'],
  queryFn: () => invoke<ModelInfo[]>('list_models'),
});

export const downloadedModelsQuery = queryOptions({
  queryKey: ['downloaded-models'],
  queryFn: () => invoke<string[]>('list_downloaded_models'),
});

export const microphonesQuery = queryOptions({
  queryKey: ['microphones'],
  queryFn: () => invoke<MicDevice[]>('list_microphones'),
});

export const permissionsQuery = queryOptions({
  queryKey: ['permissions'],
  queryFn: () => invoke<PermissionsStatus>('get_permissions_status'),
});

export const transcriptLogsQuery = queryOptions({
  queryKey: ['transcript-logs'],
  queryFn: () => invoke<TranscriptLogEntry[]>('get_transcript_logs'),
});

export const debugLogsQuery = queryOptions({
  queryKey: ['debug-logs'],
  queryFn: () => invoke<DebugLogEntry[]>('get_debug_logs'),
});

export const dictationStateQuery = queryOptions({
  queryKey: ['dictation-state'],
  queryFn: () => invoke<DictationState>('get_dictation_state'),
});

export const backendStatusQuery = queryOptions({
  queryKey: ['backend-status'],
  queryFn: () => invoke<BackendStatus>('get_backend_status'),
});

export const pipelineMetricsQuery = queryOptions({
  queryKey: ['pipeline-metrics'],
  queryFn: () => invoke<PipelineMetricsSnapshot>('get_pipeline_metrics'),
});

export const hotkeyPresetsQuery = queryOptions({
  queryKey: ['hotkey-presets'],
  queryFn: () => invoke<HotkeyPresetInfo[]>('list_hotkey_presets'),
});

export const usageStatsQuery = queryOptions({
  queryKey: ['usage-stats'],
  queryFn: () => invoke<DailyStat[]>('get_usage_stats'),
});

export const llmModelsQuery = queryOptions({
  queryKey: ['llm-models'],
  queryFn: async () => {
    try {
      return await invoke<LlmModelEntry[]>('list_llm_models');
    } catch {
      return [];
    }
  },
  staleTime: 5 * 60_000, // cache for 5 minutes – avoids repeated HTTP calls on tab navigation
});

export const remoteSpeechModelsQuery = queryOptions({
  queryKey: ['remote-speech-models'],
  queryFn: async () => {
    try {
      return await invoke<SpeechModelEntry[]>('list_remote_speech_models');
    } catch {
      return [];
    }
  },
  staleTime: 60_000,
});

// ── Mutation functions (for TanStack Query useMutation) ──

export const commands = {
  updateSettings: (patch: SettingsPatch) =>
    invoke<Settings>('update_settings', { patch }),

  downloadModel: (modelId: string) =>
    invoke<void>('download_model', { modelId }),

  deleteModel: (modelId: string) =>
    invoke<void>('delete_model', { modelId }),

  requestPermission: (kind: PermissionKind) =>
    invoke<void>('request_permission', { kind }),

  restartApp: () =>
    invoke<void>('restart_app'),

  clearTranscriptLogs: () =>
    invoke<void>('clear_transcript_logs'),

  clearTranscriptLogsAndRecordings: () =>
    invoke<void>('clear_transcript_logs_and_recordings'),

  getRecordingAudio: (filename: string) =>
    invoke<number[]>('get_recording_audio', { filename }),

  clearDebugLogs: () =>
    invoke<void>('clear_debug_logs'),

  getAudioInputLevel: () =>
    invoke<number>('get_audio_input_level'),

  debugStartDictation: () =>
    invoke<void>('debug_start_dictation'),

  debugStopDictation: () =>
    invoke<void>('debug_stop_dictation'),

  testLlmCleanupConnection: () =>
    invoke<string>('test_llm_cleanup_connection'),

  testRemoteSpeechConnection: () =>
    invoke<string>('test_remote_speech_connection'),

  enhanceText: (text: string) =>
    invoke<string>('enhance_text', { text }),

  getPipelineMetrics: () =>
    invoke<PipelineMetricsSnapshot>('get_pipeline_metrics'),

  clearPipelineMetrics: () =>
    invoke<void>('clear_pipeline_metrics'),

  purgeLocalArtifacts: () =>
    invoke<void>('purge_local_artifacts'),

  clearUsageStats: () =>
    invoke<void>('clear_usage_stats'),
} as const;

/**
 * If the saved mic_device_id no longer appears in the available device list,
 * reset it to null (system default) and return the updated settings.
 */
export async function reconcileMicSetting(
  settings: Settings,
  mics: MicDevice[],
): Promise<Settings> {
  if (
    settings.mic_device_id != null &&
    !mics.some((m) => m.id === settings.mic_device_id)
  ) {
    return commands.updateSettings({ mic_device_id: null });
  }
  return settings;
}
