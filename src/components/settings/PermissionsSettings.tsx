import { useAtomValue, useSetAtom } from 'jotai';
import { CheckCircle, XCircle, HelpCircle, Shield } from 'lucide-react';
import { Stack, Group, Text, Paper, Button, Box } from '@mantine/core';
import { SectionCard } from '../ui/SectionCard';
import { permissionsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import type { PermissionKind, PermissionState, PermissionsStatus } from '../../types';
import { usePermissionRefreshOnFocus } from '../../lib/hooks/usePermissionRefreshOnFocus';

const permRows: { kind: PermissionKind; label: string; detail: string }[] = [
  { kind: 'microphone', label: 'Microphone', detail: 'Needed to capture your voice.' },
  { kind: 'accessibility', label: 'Accessibility', detail: 'Needed to insert text into other apps.' },
  { kind: 'input_monitoring', label: 'Input Monitoring', detail: 'Needed to detect your global hotkey.' },
];

function PermIcon({ state }: { state: PermissionState }) {
  if (state === 'granted') return <CheckCircle size={18} color="var(--mantine-color-green-6)" />;
  if (state === 'denied') return <XCircle size={18} color="var(--mantine-color-red-6)" />;
  return <HelpCircle size={18} color="var(--mantine-color-dimmed)" />;
}

export function PermissionsSettings() {
  const permissions = useAtomValue(permissionsAtom);
  const setPermissions = useSetAtom(permissionsAtom);
  const armPermissionRecheck = usePermissionRefreshOnFocus();

  async function handleRequest(kind: PermissionKind) {
    armPermissionRecheck();
    try {
      await invoke('request_permission', { kind });
      const perms = await invoke<PermissionsStatus>('get_permissions_status');
      setPermissions(perms);
      addToast('success', 'Permission prompt opened.');
    } catch (error) {
      addToast('error', `Failed requesting permission: ${String(error)}`);
    }
  }

  return (
    <Box p="xl" maw={640} mx="auto">
      <SectionCard icon={Shield} title="macOS Permissions">
        <Stack gap="sm">
          <Text size="xs" c="dimmed" mb="xs">
            Grant the required macOS permissions for dictation, text insertion, and hotkeys.
          </Text>

          {permRows.map((row) => (
            <Paper key={row.kind} p="md" radius="md" withBorder>
              <Group>
                <PermIcon state={permissions[row.kind]} />
                <Stack gap={0} style={{ flex: 1 }}>
                  <Group gap="xs">
                    <Text size="sm" fw={500}>{row.label}</Text>
                    <Text size="xs" c="dimmed">{permissions[row.kind]}</Text>
                  </Group>
                  <Text size="xs" c="dimmed">{row.detail}</Text>
                </Stack>
                <Button variant="default" size="compact-xs" onClick={() => handleRequest(row.kind)}>
                  {permissions[row.kind] === 'granted' ? 'Recheck' : 'Open Settings'}
                </Button>
              </Group>
            </Paper>
          ))}
        </Stack>
      </SectionCard>
    </Box>
  );
}
