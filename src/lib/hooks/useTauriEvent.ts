import { useEffect } from 'react';
import { listen, tauriAvailable } from '../tauri';

export function useTauriEvent<T>(eventName: string, callback: (payload: T) => void): void {
  useEffect(() => {
    if (!tauriAvailable) return;

    let unlisten: (() => void) | undefined;

    listen<T>(eventName, callback).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [eventName, callback]);
}
