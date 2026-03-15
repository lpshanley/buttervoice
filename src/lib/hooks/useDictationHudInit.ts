import { useCallback, useEffect, useRef } from 'react';
import { useSetAtom } from 'jotai';
import { commands } from '../commands';
import { invoke, tauriAvailable } from '../tauri';
import { useTauriEvent } from './useTauriEvent';
import { usePolling } from './usePolling';
import { dictationStateAtom, inputLevelAtom } from '../../stores/app';
import type { DictationState } from '../../types';

export function useDictationHudInit(): void {
  const setDictationState = useSetAtom(dictationStateAtom);
  const setInputLevel = useSetAtom(inputLevelAtom);
  const stateRef = useRef<DictationState>('idle');

  useEffect(() => {
    if (!tauriAvailable) return;

    Promise.all([
      invoke<DictationState>('get_dictation_state'),
      commands.getAudioInputLevel(),
    ])
      .then(([state, level]) => {
        stateRef.current = state;
        setDictationState(state);
        setInputLevel(level);
      })
      .catch(() => {
        setInputLevel(0);
      });
  }, [setDictationState, setInputLevel]);

  useTauriEvent<DictationState>('dictation-state', useCallback((state) => {
    stateRef.current = state;
    setDictationState(state);
    if (state !== 'recording') {
      setInputLevel(0);
    }
  }, [setDictationState, setInputLevel]));

  usePolling(
    useCallback(() => {
      if (!tauriAvailable) return;
      if (stateRef.current !== 'recording') {
        return;
      }
      commands.getAudioInputLevel().then(setInputLevel).catch(() => {});
    }, [setInputLevel]),
    90,
  );
}
