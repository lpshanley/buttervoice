import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import type {
  BackendStatus,
  DictationState,
  MicDevice,
  ModelDownloadProgress,
  ModelInfo,
  PermissionsStatus,
  Settings,
} from '../types';

// ── Core app state atoms ──

export const settingsAtom = atom<Settings | null>(null);
export const dictationStateAtom = atom<DictationState>('idle');
export const permissionsAtom = atom<PermissionsStatus>({
  microphone: 'unknown',
  accessibility: 'unknown',
  input_monitoring: 'unknown',
});
export const backendStatusAtom = atom<BackendStatus | null>(null);
export const modelsAtom = atom<ModelInfo[]>([]);
export const downloadedModelsAtom = atom<Set<string>>(new Set<string>());
export const downloadingModelsAtom = atom<Set<string>>(new Set<string>());
export const deletingModelsAtom = atom<Set<string>>(new Set<string>());
export const downloadProgressAtom = atom<Map<string, ModelDownloadProgress>>(new Map());
export const microphonesAtom = atom<MicDevice[]>([]);
export const inputLevelAtom = atom<number>(0);

// ── UI state atoms ──

export const loadingAtom = atom<boolean>(true);
export const onboardingDismissedAtom = atom<boolean>(false);
export const restartPromptVisibleAtom = atom<boolean>(false);
export const restartingAppAtom = atom<boolean>(false);
export const testingLlmConnectionAtom = atom<boolean>(false);
export const testingRemoteSpeechConnectionAtom = atom<boolean>(false);
export const copyingLastTranscriptAtom = atom<boolean>(false);

// ── Derived atoms ──

export const hasAllPermissionsAtom = atom((get) => {
  const perms = get(permissionsAtom);
  return (
    perms.microphone === 'granted' &&
    perms.accessibility === 'granted' &&
    perms.input_monitoring === 'granted'
  );
});

export const hasModelDownloadedAtom = atom((get) => get(downloadedModelsAtom).size > 0);

export const hasSpeechProviderConfiguredAtom = atom((get) => {
  const settings = get(settingsAtom);
  if (!settings) return false;
  if (settings.speech_provider === 'local_whispercpp') {
    return get(hasModelDownloadedAtom);
  }
  return (
    settings.speech_remote_base_url.trim().length > 0 &&
    settings.speech_remote_model.trim().length > 0
  );
});

export const setupCompleteAtom = atom(
  (get) => get(hasAllPermissionsAtom) && get(hasSpeechProviderConfiguredAtom),
);

export const showOnboardingAtom = atom(
  (get) => !get(setupCompleteAtom) && !get(onboardingDismissedAtom) && !get(loadingAtom),
);

export const onboardingStepAtom = atom((get) => {
  if (!get(hasAllPermissionsAtom)) return 'permissions' as const;
  if (!get(hasSpeechProviderConfiguredAtom)) return 'model' as const;
  return 'ready' as const;
});

export const selectedModelIdAtom = atom((get) => get(settingsAtom)?.model_id ?? null);

export const selectedModelInfoAtom = atom((get) => {
  const id = get(selectedModelIdAtom);
  if (!id) return null;
  return get(modelsAtom).find((m) => m.id === id) ?? null;
});

export const recommendedModelAtom = atom(
  (get) => get(modelsAtom).find((m) => m.recommended) ?? null,
);

export const setupIssueLabelAtom = atom((get) => {
  if (!get(hasAllPermissionsAtom)) return 'macOS permissions needed';
  if (!get(hasSpeechProviderConfiguredAtom)) {
    const settings = get(settingsAtom);
    return settings?.speech_provider === 'remote_openai_compatible'
      ? 'configure remote speech'
      : 'download a model';
  }
  return null;
});

export const canOpenDebugTabAtom = atom((get) => Boolean(get(settingsAtom)?.debug_logging));

export const stateLabels: Record<DictationState, string> = {
  idle: 'Ready',
  recording: 'Recording',
  transcribing: 'Transcribing',
  post_processing: 'Processing',
  injecting: 'Injecting Text',
  error: 'Error',
};

// ── Pipeline live-view atoms ──

export const showLivePipelineAtom = atomWithStorage<boolean>('buttervoice:show-live-pipeline', true);

/** Maps DictationState → stepper step index.
 *  Steps: 0=Recording, 1=Transcription, 2=Processing, 3=Output.
 *  Returns 4 when all steps are complete (idle after a run).
 *  Returns -1 for error or unknown states. */
export const pipelineStepIndexAtom = atom((get) => {
  const state = get(dictationStateAtom);
  switch (state) {
    case 'recording': return 0;
    case 'transcribing': return 1;
    case 'post_processing': return 2;
    case 'injecting': return 3;
    case 'idle': return 4;
    case 'error': return -1;
    default: return -1;
  }
});
