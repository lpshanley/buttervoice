import { useEffect, useCallback } from 'react';
import { useSetAtom } from 'jotai';
import { useQueryClient } from '@tanstack/react-query';
import { tauriAvailable } from '../tauri';
import {
  settingsAtom,
  dictationStateAtom,
  permissionsAtom,
  backendStatusAtom,
  modelsAtom,
  downloadedModelsAtom,
  microphonesAtom,
  loadingAtom,
} from '../../stores/app';
import { transcriptLogsAtom, debugLogsAtom } from '../../stores/logs';
import { addToast } from '../../stores/toasts';
import {
  settingsQuery,
  modelsQuery,
  downloadedModelsQuery,
  microphonesQuery,
  permissionsQuery,
  transcriptLogsQuery,
  debugLogsQuery,
  dictationStateQuery,
  backendStatusQuery,
  reconcileMicSetting,
} from '../commands';
import { useTauriEvent } from './useTauriEvent';
import { usePolling } from './usePolling';
import { invoke } from '../tauri';
import type { DictationState, DebugLogEntry, MicDevice, Settings, TranscriptLogEntry } from '../../types';

export function useAppInit(): void {
  const queryClient = useQueryClient();

  const setSettings = useSetAtom(settingsAtom);
  const setDictationState = useSetAtom(dictationStateAtom);
  const setPermissions = useSetAtom(permissionsAtom);
  const setBackendStatus = useSetAtom(backendStatusAtom);
  const setModels = useSetAtom(modelsAtom);
  const setDownloadedModels = useSetAtom(downloadedModelsAtom);
  const setMicrophones = useSetAtom(microphonesAtom);
  const setLoading = useSetAtom(loadingAtom);
  const setTranscriptLogs = useSetAtom(transcriptLogsAtom);
  const setDebugLogs = useSetAtom(debugLogsAtom);
  // ── Initial data load ──
  useEffect(() => {
    if (!tauriAvailable) {
      setLoading(false);
      addToast('error', 'Tauri runtime not detected. Run this view through `cargo tauri dev`.');
      return;
    }

    setLoading(true);

    Promise.all([
      queryClient.fetchQuery(settingsQuery),
      queryClient.fetchQuery(modelsQuery),
      queryClient.fetchQuery(downloadedModelsQuery),
      queryClient.fetchQuery(microphonesQuery),
      queryClient.fetchQuery(permissionsQuery),
      queryClient.fetchQuery(transcriptLogsQuery),
      queryClient.fetchQuery(debugLogsQuery),
      queryClient.fetchQuery(dictationStateQuery),
      queryClient.fetchQuery(backendStatusQuery),
    ])
      .then(async ([settings, models, downloaded, mics, perms, logs, debugLogs, state, backend]) => {
        setModels(models);
        setDownloadedModels(new Set(downloaded));
        setMicrophones(mics);
        setPermissions(perms);
        setTranscriptLogs(logs);
        setDebugLogs(debugLogs);
        setDictationState(state);
        setBackendStatus(backend);

        // If the saved mic is no longer available, revert to system default.
        const resolved = await reconcileMicSetting(settings, mics);
        setSettings(resolved);
      })
      .catch((error) => {
        addToast('error', `Failed loading app state: ${String(error)}`);
      })
      .finally(() => {
        setLoading(false);
      });
  }, [
    queryClient,
    setSettings,
    setModels,
    setDownloadedModels,
    setMicrophones,
    setPermissions,
    setTranscriptLogs,
    setDebugLogs,
    setDictationState,
    setBackendStatus,
    setLoading,
  ]);

  // ── Tauri event subscriptions ──
  useTauriEvent<DictationState>('dictation-state', useCallback((state) => {
    setDictationState(state);
  }, [setDictationState]));

  useTauriEvent<TranscriptLogEntry>('transcript-log-added', useCallback((entry) => {
    setTranscriptLogs((prev) => [...prev, entry]);
  }, [setTranscriptLogs]));

  useTauriEvent<TranscriptLogEntry>('transcript-log-updated', useCallback((entry) => {
    setTranscriptLogs((prev) => {
      if (prev.some((existing) => existing.request_id === entry.request_id)) {
        return prev.map((existing) =>
          existing.request_id === entry.request_id ? entry : existing,
        );
      }
      return [...prev, entry];
    });
  }, [setTranscriptLogs]));

  useTauriEvent('transcript-logs-cleared', useCallback(() => {
    setTranscriptLogs([]);
  }, [setTranscriptLogs]));

  useTauriEvent<DebugLogEntry>('debug-log-added', useCallback((entry) => {
    setDebugLogs((prev) => [...prev, entry]);
  }, [setDebugLogs]));

  useTauriEvent('debug-logs-cleared', useCallback(() => {
    setDebugLogs([]);
  }, [setDebugLogs]));

  useTauriEvent<Settings>('mic-device-reset', useCallback((settings) => {
    setSettings(settings);
    addToast('info', 'Selected microphone is no longer available. Reverted to system default.');
    // Refresh the device list so the dropdown is up to date.
    invoke<MicDevice[]>('list_microphones')
      .then(setMicrophones)
      .catch(() => {});
  }, [setSettings, setMicrophones]));

  // ── Polling for runtime state ──
  usePolling(
    useCallback(() => {
      if (!tauriAvailable) return;
      Promise.all([
        queryClient.fetchQuery({ ...dictationStateQuery, staleTime: 0 }),
        queryClient.fetchQuery({ ...transcriptLogsQuery, staleTime: 0 }),
        queryClient.fetchQuery({ ...backendStatusQuery, staleTime: 0 }),
      ])
        .then(([state, logs, backend]) => {
          setDictationState(state);
          setTranscriptLogs(logs);
        setBackendStatus(backend);
        })
        .catch(() => {});
    }, [queryClient, setDictationState, setTranscriptLogs, setBackendStatus]),
    500,
  );
}
