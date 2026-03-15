import { useAtomValue, useSetAtom } from 'jotai';
import { Sun, Cpu, Monitor } from 'lucide-react';
import { Stack, SimpleGrid, Text, Paper, UnstyledButton } from '@mantine/core';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import type { ComputeMode, Settings } from '../../types';

const computeOptions: { value: ComputeMode; label: string; summary: string; detail: string; Icon: typeof Cpu }[] = [
  { value: 'auto', label: 'Auto', summary: 'Balanced default', detail: 'Starts on GPU when available and falls back to CPU automatically.', Icon: Sun },
  { value: 'cpu', label: 'CPU', summary: 'Maximum compatibility', detail: 'Best stability choice when you prefer predictable CPU-only execution.', Icon: Cpu },
  { value: 'gpu', label: 'GPU (fallback to CPU)', summary: 'Fastest path on supported Macs', detail: 'Prefers GPU acceleration and seamlessly falls back to CPU if needed.', Icon: Monitor },
];

export function RuntimeSettings() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);

  if (!settings) return null;

  const activeOption = computeOptions.find((o) => o.value === settings.compute_mode) ?? computeOptions[0];

  async function updateMode(mode: ComputeMode) {
    if (!settings || settings.compute_mode === mode) return;
    try {
      const updated = await invoke<Settings>('update_settings', { patch: { compute_mode: mode } });
      setSettings(updated);
      addToast('success', 'Compute mode updated.');
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  return (
    <Stack gap="md">
      <Text size="xs" fw={500} c="dimmed">Compute Mode</Text>
      <SimpleGrid cols={3}>
        {computeOptions.map((option) => {
          const Icon = option.Icon;
          const isSelected = settings.compute_mode === option.value;
          return (
            <UnstyledButton
              key={option.value}
              onClick={() => updateMode(option.value)}
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
                  <Text size="sm" fw={500}>{option.label}</Text>
                  <Text size="xs" c="dimmed">{option.summary}</Text>
                </Stack>
              </Paper>
            </UnstyledButton>
          );
        })}
      </SimpleGrid>

      <Text size="xs" c="dimmed">{activeOption.detail}</Text>
    </Stack>
  );
}
