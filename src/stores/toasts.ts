import { notifications } from '@mantine/notifications';
import type { ToastKind } from '../types';

const kindColorMap: Record<ToastKind, string> = {
  info: 'blue',
  success: 'green',
  error: 'red',
};

/**
 * Show a Mantine notification.
 *
 * The `_setToasts` parameter is kept for backwards-compat with existing call
 * sites but is no longer used — Mantine manages notification state internally.
 */
export function addToast(
  kind: ToastKind,
  message: string,
  _setToasts?: unknown,
  durationMs = 4000,
): void {
  notifications.show({
    message,
    color: kindColorMap[kind],
    autoClose: durationMs > 0 ? durationMs : false,
  });
}

/**
 * Remove all currently visible notifications.
 * Call this where the old code did `setToasts(prev => prev.filter(...))`.
 */
export function clearToasts(): void {
  notifications.cleanQueue();
  notifications.clean();
}
