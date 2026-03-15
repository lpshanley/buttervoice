import { useAtomValue, useSetAtom } from 'jotai';
import { Stack, Group, Text } from '@mantine/core';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings } from '../../types';

export function BetaSettings() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);

  if (!settings) return null;

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
      <Text size="xs" c="dimmed">
        These flags expose in-progress features in Settings and the studio. They default off for new installs and migrations.
      </Text>

      <Group justify="space-between">
        <div>
          <Text size="sm" fw={500}>AI Enhancement</Text>
          <Text size="xs" c="dimmed">Show AI cleanup and enhancement UI.</Text>
        </div>
        <Switch
          checked={settings.beta_ai_enhancement_enabled}
          onChange={(value) =>
            applyPatch(
              { beta_ai_enhancement_enabled: value },
              `AI Enhancement beta ${value ? 'enabled' : 'disabled'}.`,
            )
          }
          label="Toggle AI Enhancement beta"
        />
      </Group>

      <Group justify="space-between">
        <div>
          <Text size="sm" fw={500}>Classification</Text>
          <Text size="xs" c="dimmed">Show content classification settings and badges.</Text>
        </div>
        <Switch
          checked={settings.beta_content_classification_enabled}
          onChange={(value) =>
            applyPatch(
              { beta_content_classification_enabled: value },
              `Classification beta ${value ? 'enabled' : 'disabled'}.`,
            )
          }
          label="Toggle Classification beta"
        />
      </Group>

      <Group justify="space-between">
        <div>
          <Text size="sm" fw={500}>Personas</Text>
          <Text size="xs" c="dimmed">Show persona management and transformation UI.</Text>
        </div>
        <Switch
          checked={settings.beta_personas_enabled}
          onChange={(value) =>
            applyPatch(
              { beta_personas_enabled: value },
              `Personas beta ${value ? 'enabled' : 'disabled'}.`,
            )
          }
          label="Toggle Personas beta"
        />
      </Group>
    </Stack>
  );
}
