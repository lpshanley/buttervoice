import { useAtomValue, useSetAtom } from 'jotai';
import { SegmentedControl, Stack, Text } from '@mantine/core';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import type { OutputDestination, Settings } from '../../types';

export function OutputSettings() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);

  if (!settings) return null;

  async function change(value: string) {
    try {
      const dest = value as OutputDestination;
      const updated = await invoke<Settings>('update_settings', { patch: { output_destination: dest } });
      setSettings(updated);
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  return (
    <Stack gap="xs">
      <div>
        <Text size="sm" fw={500}>Output destination</Text>
        <Text size="xs" c="dimmed">
          Choose where transcribed text is sent after processing.
        </Text>
      </div>
      <SegmentedControl
        size="xs"
        value={settings.output_destination}
        onChange={change}
        data={[
          { label: 'Type to Input', value: 'input' },
          { label: 'Clipboard', value: 'clipboard' },
          { label: 'None', value: 'none' },
        ]}
      />
    </Stack>
  );
}
