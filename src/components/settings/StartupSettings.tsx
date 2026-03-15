import { useAtomValue, useSetAtom } from 'jotai';
import { Group, Text } from '@mantine/core';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings } from '../../types';

export function StartupSettings() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);

  if (!settings) return null;

  async function toggle(value: boolean) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch: { launch_at_login: value } });
      setSettings(updated);
      addToast('success', 'Launch-at-login updated.');
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  return (
    <Group justify="space-between">
      <div>
        <Text size="sm" fw={500}>Launch at login</Text>
        <Text size="xs" c="dimmed">Open ButterVoice automatically when you sign in.</Text>
      </div>
      <Switch
        checked={settings.launch_at_login}
        onChange={toggle}
        label="Toggle launch at login"
      />
    </Group>
  );
}
