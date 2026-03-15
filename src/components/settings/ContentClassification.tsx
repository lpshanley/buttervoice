import { useEffect, useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Stack,
  Group,
  Text,
  Textarea,
  Slider,
  Collapse,
  ActionIcon,
  Tooltip,
  Box,
  Alert,
} from '@mantine/core';
import { Eye, EyeOff, ShieldAlert } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { ModelOverrideSelect } from '../ui/ModelOverrideSelect';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings, SettingsPatch } from '../../types';

export function ContentClassification() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);

  const [defaultPrompt, setDefaultPrompt] = useState('');
  const [showDefaultPrompt, setShowDefaultPrompt] = useState(false);

  useEffect(() => {
    invoke<string>('get_default_classification_prompt').then(setDefaultPrompt).catch(() => {});
  }, []);

  if (!settings) return null;

  if (!settings.beta_content_classification_enabled) {
    return (
      <Box p="xl" maw={640} mx="auto">
        <Alert color="blue" variant="light">
          Classification is currently behind a beta flag. Enable it in Advanced &gt; Beta to use this feature.
        </Alert>
      </Box>
    );
  }

  async function applyPatch(patch: SettingsPatch, msg?: string) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch });
      setSettings(updated);
      if (msg) addToast('success', msg);
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  return (
    <Box p="xl" maw={640} mx="auto">
      <Stack gap="lg">
        <SectionCard icon={ShieldAlert} title="Content Classification">
          <Stack gap="md">
            <Group justify="space-between">
              <div>
                <Text size="sm" fw={500}>Enable Content Classification</Text>
                <Text size="xs" c="dimmed">Show classification results in the studio</Text>
              </div>
              <Switch
                checked={settings.content_classification_enabled}
                onChange={(v) =>
                  applyPatch(
                    { content_classification_enabled: v },
                    `Content classification ${v ? 'enabled' : 'disabled'}.`,
                  )
                }
                label="Toggle content classification"
              />
            </Group>

            <Group justify="space-between">
              <div>
                <Text size="sm" fw={500}>Auto-Apply Classification</Text>
                <Text size="xs" c="dimmed">Automatically classify text on every dictation</Text>
              </div>
              <Switch
                checked={settings.content_classification_auto_apply}
                onChange={(v) =>
                  applyPatch(
                    { content_classification_auto_apply: v },
                    `Classification auto-apply ${v ? 'enabled' : 'disabled'}.`,
                  )
                }
                disabled={!settings.content_classification_enabled}
                label="Toggle auto-apply"
              />
            </Group>

            <ModelOverrideSelect
              value={settings.content_classification_model_override}
              onChange={(val) =>
                applyPatch(
                  { content_classification_model_override: val },
                  val ? 'Classification model override set.' : 'Classification model override cleared.',
                )
              }
              label="Classification Model Override"
            />

            <div>
              <Text size="sm" fw={500} mb={4}>Warning Threshold</Text>
              <Text size="xs" c="dimmed" mb="xs">
                Score at which a warning badge is shown ({settings.content_classification_warning_threshold.toFixed(2)})
              </Text>
              <Slider
                min={0}
                max={1}
                step={0.05}
                value={settings.content_classification_warning_threshold}
                onChange={(v) => applyPatch({ content_classification_warning_threshold: v })}
                marks={[
                  { value: 0, label: '0' },
                  { value: 0.5, label: '0.5' },
                  { value: 1, label: '1' },
                ]}
              />
            </div>

            <div>
              <Text size="sm" fw={500} mb={4}>Block Threshold</Text>
              <Text size="xs" c="dimmed" mb="xs">
                Score at which auto-injection is blocked ({settings.content_classification_block_threshold.toFixed(2)})
              </Text>
              <Slider
                min={0}
                max={1}
                step={0.05}
                value={settings.content_classification_block_threshold}
                onChange={(v) => applyPatch({ content_classification_block_threshold: v })}
                marks={[
                  { value: 0, label: '0' },
                  { value: 0.5, label: '0.5' },
                  { value: 1, label: '1' },
                ]}
              />
            </div>
          </Stack>
        </SectionCard>

        <SectionCard icon={ShieldAlert} title="Classification Prompt">
          <Stack gap="md">
            <Group justify="space-between">
              <Text size="sm" fw={500}>Use Custom Classification Prompt</Text>
              <Switch
                checked={settings.content_classification_use_custom_prompt}
                onChange={(v) =>
                  applyPatch(
                    { content_classification_use_custom_prompt: v },
                    `Custom classification prompt ${v ? 'enabled' : 'disabled'}.`,
                  )
                }
                label="Override the built-in classification prompt"
              />
            </Group>

            <Textarea
              label="Custom Classification Prompt"
              size="sm"
              autosize
              minRows={4}
              maxRows={14}
              defaultValue={settings.content_classification_custom_prompt}
              placeholder="Enter a custom classification prompt…"
              disabled={!settings.content_classification_use_custom_prompt}
              onBlur={(e) =>
                applyPatch(
                  { content_classification_custom_prompt: e.target.value },
                  'Custom classification prompt updated.',
                )
              }
            />

            <Group gap="xs">
              <Tooltip label={showDefaultPrompt ? 'Hide default prompt' : 'View default prompt'}>
                <ActionIcon
                  variant="subtle"
                  size="sm"
                  onClick={() => setShowDefaultPrompt((v) => !v)}
                >
                  {showDefaultPrompt ? <EyeOff size={14} /> : <Eye size={14} />}
                </ActionIcon>
              </Tooltip>
              <Text
                size="xs"
                c="dimmed"
                style={{ cursor: 'pointer' }}
                onClick={() => setShowDefaultPrompt((v) => !v)}
              >
                {showDefaultPrompt ? 'Hide' : 'View'} default classification prompt
              </Text>
            </Group>

            <Collapse in={showDefaultPrompt}>
              <Textarea
                size="sm"
                autosize
                minRows={4}
                maxRows={14}
                value={defaultPrompt}
                readOnly
                styles={{
                  input: {
                    opacity: 0.7,
                    fontFamily: 'monospace',
                    fontSize: 'var(--mantine-font-size-xs)',
                  },
                }}
              />
            </Collapse>
          </Stack>
        </SectionCard>
      </Stack>
    </Box>
  );
}
