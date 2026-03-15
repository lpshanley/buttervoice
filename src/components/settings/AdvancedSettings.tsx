import { useAtomValue, useSetAtom } from 'jotai';
import { Link } from '@tanstack/react-router';
import { Stack, Group, Text, Button, NumberInput } from '@mantine/core';
import { settingsAtom, canOpenDebugTabAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings } from '../../types';

export function AdvancedSettings() {
  const settings = useAtomValue(settingsAtom);
  const canOpenDebug = useAtomValue(canOpenDebugTabAtom);
  const setSettings = useSetAtom(settingsAtom);

  if (!settings) return null;

  async function toggle(value: boolean) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch: { debug_logging: value } });
      setSettings(updated);
      addToast('success', value ? 'Debug logging enabled.' : 'Debug logging disabled.');
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  async function applyPatch(patch: Partial<Settings>, success: string) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch });
      setSettings(updated);
      addToast('success', success);
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  return (
    <Stack gap="md">
      <Group justify="space-between">
        <Text size="sm" fw={500}>Enable debug logging</Text>
        <Switch
          checked={settings.debug_logging}
          onChange={toggle}
          label="Toggle debug logging"
        />
      </Group>

      <Text size="xs" c="dimmed">
        Streams trace events to the Debug tab and stderr for text emission, LLM cleanup, and input injection.
      </Text>

      <Group justify="space-between">
        <Text size="sm" fw={500}>Include transcript content in debug logs</Text>
        <Switch
          checked={settings.debug_log_include_content}
          onChange={(value) => applyPatch({ debug_log_include_content: value }, 'Debug content setting updated.')}
          label="Toggle content logging"
        />
      </Group>

      <NumberInput
        label="Recording retention (hours)"
        size="sm"
        min={1}
        max={24 * 30}
        value={settings.recording_retention_hours}
        onChange={(v) => {
          if (typeof v === 'number' && Number.isFinite(v)) {
            applyPatch({ recording_retention_hours: Math.round(v) }, 'Recording retention updated.');
          }
        }}
      />

      <NumberInput
        label="Debug retention (hours)"
        size="sm"
        min={1}
        max={24 * 30}
        value={settings.debug_log_retention_hours}
        onChange={(v) => {
          if (typeof v === 'number' && Number.isFinite(v)) {
            applyPatch({ debug_log_retention_hours: Math.round(v) }, 'Debug retention updated.');
          }
        }}
      />

      {canOpenDebug && (
        <Button variant="default" size="xs" component={Link} to="/debug" style={{ alignSelf: 'flex-start' }}>
          Open Debug Tab
        </Button>
      )}
    </Stack>
  );
}
