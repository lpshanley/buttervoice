import { useAtomValue, useSetAtom } from 'jotai';
import { Mic, ToggleLeft } from 'lucide-react';
import { Stack, Select, SimpleGrid, Text, Paper, UnstyledButton } from '@mantine/core';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import type { DictationMode, HotkeyKey, HotkeyPreset, Settings } from '../../types';
import { HOTKEY_LABELS } from '../../types';

const hotkeyOptions: { value: HotkeyPreset; label: string }[] = [
  { value: 'right_option', label: HOTKEY_LABELS.right_option },
  { value: 'left_option', label: HOTKEY_LABELS.left_option },
  { value: 'right_command', label: HOTKEY_LABELS.right_command },
  { value: 'right_control', label: HOTKEY_LABELS.right_control },
  { value: 'left_control', label: HOTKEY_LABELS.left_control },
  { value: 'fn', label: HOTKEY_LABELS.fn },
];

const dictationModeOptions: {
  value: DictationMode;
  label: string;
  summary: string;
  detail: string;
  Icon: typeof Mic;
}[] = [
  {
    value: 'push_to_talk',
    label: 'Push to Talk',
    summary: 'Hold to record',
    detail: 'Hold the hotkey to record, release to stop and transcribe.',
    Icon: Mic,
  },
  {
    value: 'toggle',
    label: 'Toggle',
    summary: 'Press to start/stop',
    detail: 'Press once to start recording, press again to stop and transcribe.',
    Icon: ToggleLeft,
  },
];

export function HotkeySettings() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);

  if (!settings) return null;

  const currentHotkey = typeof settings.hotkey === 'string' ? settings.hotkey : null;
  const activeMode =
    dictationModeOptions.find((o) => o.value === settings.dictation_mode) ??
    dictationModeOptions[0];

  async function updateHotkey(value: string | null) {
    if (!value) return;
    try {
      const updated = await invoke<Settings>('update_settings', {
        patch: { hotkey: value as HotkeyKey },
      });
      setSettings(updated);
      addToast('success', 'Hotkey updated.');
    } catch (error) {
      addToast('error', `Failed to update hotkey: ${String(error)}`);
    }
  }

  async function updateDictationMode(mode: DictationMode) {
    if (!settings || settings.dictation_mode === mode) return;
    try {
      const updated = await invoke<Settings>('update_settings', {
        patch: { dictation_mode: mode },
      });
      setSettings(updated);
      addToast('success', 'Dictation mode updated.');
    } catch (error) {
      addToast('error', `Failed to update dictation mode: ${String(error)}`);
    }
  }

  return (
    <Stack gap="md">
      <Select
        label="Hotkey"
        description="Global key to trigger dictation"
        size="sm"
        data={hotkeyOptions}
        value={currentHotkey}
        onChange={updateHotkey}
        allowDeselect={false}
      />

      <Text size="xs" fw={500} c="dimmed">
        Dictation Mode
      </Text>
      <SimpleGrid cols={2}>
        {dictationModeOptions.map((option) => {
          const Icon = option.Icon;
          const isSelected = settings.dictation_mode === option.value;
          return (
            <UnstyledButton
              key={option.value}
              onClick={() => updateDictationMode(option.value)}
            >
              <Paper
                p="md"
                radius="md"
                withBorder
                style={{
                  borderColor: isSelected ? 'var(--mantine-color-blue-6)' : undefined,
                  backgroundColor: isSelected ? 'var(--mantine-color-blue-light)' : undefined,
                  cursor: 'pointer',
                }}
              >
                <Stack gap={4} align="center" ta="center">
                  <Icon size={16} color="var(--mantine-color-dimmed)" />
                  <Text size="sm" fw={500}>
                    {option.label}
                  </Text>
                  <Text size="xs" c="dimmed">
                    {option.summary}
                  </Text>
                </Stack>
              </Paper>
            </UnstyledButton>
          );
        })}
      </SimpleGrid>

      <Text size="xs" c="dimmed">
        {activeMode.detail}
      </Text>
    </Stack>
  );
}
