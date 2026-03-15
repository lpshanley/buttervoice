import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';

export type UnlistenFn = () => void;

type TauriWindow = Window & {
  __TAURI__?: unknown;
  __TAURI_INTERNALS__?: unknown;
};

export const tauriAvailable =
  typeof window !== 'undefined' &&
  Boolean((window as TauriWindow).__TAURI__ || (window as TauriWindow).__TAURI_INTERNALS__);

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, args);
}

export async function listen<T>(eventName: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return tauriListen<T>(eventName, (event) => cb(event.payload));
}
