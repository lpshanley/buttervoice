import { useCallback, useEffect, useRef } from 'react';
import { useSetAtom } from 'jotai';
import { permissionsAtom } from '../../stores/app';
import { invoke } from '../tauri';
import type { PermissionsStatus } from '../../types';

/**
 * Arms a one-shot permission recheck that fires the next time the window
 * regains focus (e.g. after the user switches to macOS Settings and back).
 *
 * Call the returned `arm` function right before opening system settings.
 */
export function usePermissionRefreshOnFocus() {
  const setPermissions = useSetAtom(permissionsAtom);
  const armed = useRef(false);

  const arm = useCallback(() => {
    armed.current = true;
  }, []);

  useEffect(() => {
    function onFocus() {
      if (!armed.current) return;
      armed.current = false;
      invoke<PermissionsStatus>('get_permissions_status')
        .then(setPermissions)
        .catch(() => {});
    }

    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [setPermissions]);

  return arm;
}
