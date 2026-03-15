import { atom } from 'jotai';
import type { DebugLogEntry, TranscriptLogEntry } from '../types';

export const HISTORY_LOG_LIMIT = 200;

export const transcriptLogsAtom = atom<TranscriptLogEntry[]>([]);
export const debugLogsAtom = atom<DebugLogEntry[]>([]);

// ── Derived atoms ──

export const sortedLogsAtom = atom((get) =>
  [...get(transcriptLogsAtom)].sort((a, b) => b.timestamp_ms - a.timestamp_ms),
);

export const latestInfoOperationAtom = atom(
  (get) => get(sortedLogsAtom).find((entry) => entry.is_final) ?? null,
);

export const debugLogTailAtom = atom((get) => get(debugLogsAtom).slice(-400));

export const historyLogTailAtom = atom((get) =>
  [...get(transcriptLogsAtom)]
    .sort((a, b) => a.timestamp_ms - b.timestamp_ms)
    .slice(-HISTORY_LOG_LIMIT),
);

export const lastTranscriptEntryAtom = atom(
  (get) =>
    get(sortedLogsAtom).find(
      (entry) => !entry.text.startsWith('[error]') && entry.text.trim().length > 0,
    ) ?? null,
);

export const lastTranscriptTextAtom = atom(
  (get) => get(lastTranscriptEntryAtom)?.text ?? '',
);
