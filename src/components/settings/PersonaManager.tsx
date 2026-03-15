import { useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Stack,
  Group,
  Text,
  TextInput,
  Textarea,
  Button,
  Badge,
  Paper,
  ActionIcon,
  Tooltip,
  Box,
  Modal,
  Alert,
} from '@mantine/core';
import { UserPen, Plus, Trash2, RotateCcw } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { ModelOverrideSelect } from '../ui/ModelOverrideSelect';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings, SettingsPatch, Persona } from '../../types';

export function PersonaManager() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);
  const [deleteTarget, setDeleteTarget] = useState<Persona | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newName, setNewName] = useState('');
  const [newPrompt, setNewPrompt] = useState('');

  if (!settings) return null;

  if (!settings.beta_personas_enabled) {
    return (
      <Box p="xl" maw={640} mx="auto">
        <Alert color="blue" variant="light">
          Personas are currently behind a beta flag. Enable them in Advanced &gt; Beta to use this feature.
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

  async function updatePersona(persona: Persona) {
    try {
      const updated = await invoke<Settings>('update_persona', { persona });
      setSettings(updated);
    } catch (error) {
      addToast('error', `Failed to update persona: ${String(error)}`);
    }
  }

  async function addPersona() {
    if (!newName.trim()) return;
    const persona: Persona = {
      id: `custom-${Date.now()}`,
      name: newName.trim(),
      system_prompt: newPrompt,
      model_override: '',
      is_default: false,
    };
    try {
      const updated = await invoke<Settings>('add_persona', { persona });
      setSettings(updated);
      setNewName('');
      setNewPrompt('');
      setShowAddForm(false);
      addToast('success', `Persona "${persona.name}" added.`);
    } catch (error) {
      addToast('error', `Failed to add persona: ${String(error)}`);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    try {
      const updated = await invoke<Settings>('delete_persona', { personaId: deleteTarget.id });
      setSettings(updated);
      addToast('success', `Persona "${deleteTarget.name}" deleted.`);
    } catch (error) {
      addToast('error', `Failed to delete persona: ${String(error)}`);
    }
    setDeleteTarget(null);
  }

  async function resetPrompt(persona: Persona) {
    try {
      const defaultPrompt = await invoke<string>('get_default_persona_prompt', { personaId: persona.id });
      await updatePersona({ ...persona, system_prompt: defaultPrompt });
      addToast('success', `Prompt for "${persona.name}" reset to default.`);
    } catch (error) {
      addToast('error', `Failed to reset prompt: ${String(error)}`);
    }
  }

  return (
    <Box p="xl" maw={640} mx="auto">
      <Stack gap="lg">
        <SectionCard icon={UserPen} title="Personas">
          <Stack gap="md">
            <Group justify="space-between">
              <div>
                <Text size="sm" fw={500}>Enable Personas</Text>
                <Text size="xs" c="dimmed">Show persona features in the studio</Text>
              </div>
              <Switch
                checked={settings.persona_enabled}
                onChange={(v) =>
                  applyPatch(
                    { persona_enabled: v },
                    `Personas ${v ? 'enabled' : 'disabled'}.`,
                  )
                }
                label="Toggle persona features"
              />
            </Group>

            <Group justify="space-between">
              <div>
                <Text size="sm" fw={500}>Auto-Apply Persona</Text>
                <Text size="xs" c="dimmed">Automatically run the active persona on every dictation</Text>
              </div>
              <Switch
                checked={settings.persona_auto_apply}
                onChange={(v) =>
                  applyPatch(
                    { persona_auto_apply: v },
                    `Persona auto-apply ${v ? 'enabled' : 'disabled'}.`,
                  )
                }
                disabled={!settings.persona_enabled}
                label="Toggle auto-apply"
              />
            </Group>

            <div>
              <Text size="sm" fw={500} mb={4}>Active Persona</Text>
              <select
                value={settings.persona_active_id}
                onChange={(e) =>
                  applyPatch({ persona_active_id: e.target.value }, 'Active persona updated.')
                }
                style={{
                  width: '100%',
                  padding: '6px 10px',
                  borderRadius: 'var(--mantine-radius-sm)',
                  border: '1px solid var(--mantine-color-default-border)',
                  background: 'var(--mantine-color-body)',
                  color: 'var(--mantine-color-text)',
                  fontSize: 'var(--mantine-font-size-sm)',
                }}
              >
                {settings.personas.map((p) => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
            </div>
          </Stack>
        </SectionCard>

        {settings.personas.map((persona) => (
          <Paper key={persona.id} p="md" withBorder radius="md">
            <Stack gap="sm">
              <Group justify="space-between">
                <Group gap="xs">
                  <Text size="sm" fw={600}>{persona.name}</Text>
                  {persona.is_default && (
                    <Badge size="xs" variant="light" color="blue">Built-in</Badge>
                  )}
                  {persona.id === settings.persona_active_id && (
                    <Badge size="xs" variant="filled" color="green">Active</Badge>
                  )}
                </Group>
                <Group gap={4}>
                  {persona.is_default && (
                    <Tooltip label="Reset to default prompt">
                      <ActionIcon
                        variant="subtle"
                        size="sm"
                        onClick={() => resetPrompt(persona)}
                      >
                        <RotateCcw size={14} />
                      </ActionIcon>
                    </Tooltip>
                  )}
                  {!persona.is_default && (
                    <Tooltip label="Delete persona">
                      <ActionIcon
                        variant="subtle"
                        size="sm"
                        color="red"
                        onClick={() => setDeleteTarget(persona)}
                      >
                        <Trash2 size={14} />
                      </ActionIcon>
                    </Tooltip>
                  )}
                </Group>
              </Group>

              <TextInput
                label="Name"
                size="sm"
                defaultValue={persona.name}
                onBlur={(e) => {
                  const val = e.target.value.trim();
                  if (val && val !== persona.name) {
                    updatePersona({ ...persona, name: val });
                  }
                }}
              />

              <Textarea
                label="System Prompt"
                size="sm"
                autosize
                minRows={3}
                maxRows={12}
                defaultValue={persona.system_prompt}
                onBlur={(e) => {
                  if (e.target.value !== persona.system_prompt) {
                    updatePersona({ ...persona, system_prompt: e.target.value });
                  }
                }}
              />

              <ModelOverrideSelect
                value={persona.model_override}
                onChange={(val) => updatePersona({ ...persona, model_override: val })}
              />
            </Stack>
          </Paper>
        ))}

        {showAddForm ? (
          <Paper p="md" withBorder radius="md">
            <Stack gap="sm">
              <Text size="sm" fw={600}>New Persona</Text>
              <TextInput
                label="Name"
                size="sm"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="My Custom Persona"
              />
              <Textarea
                label="System Prompt"
                size="sm"
                autosize
                minRows={3}
                maxRows={12}
                value={newPrompt}
                onChange={(e) => setNewPrompt(e.target.value)}
                placeholder="Instructions for transforming text..."
              />
              <Group>
                <Button size="xs" onClick={addPersona} disabled={!newName.trim()}>
                  Save
                </Button>
                <Button
                  size="xs"
                  variant="default"
                  onClick={() => { setShowAddForm(false); setNewName(''); setNewPrompt(''); }}
                >
                  Cancel
                </Button>
              </Group>
            </Stack>
          </Paper>
        ) : (
          <Button
            variant="default"
            size="sm"
            leftSection={<Plus size={14} />}
            onClick={() => setShowAddForm(true)}
            style={{ alignSelf: 'flex-start' }}
          >
            Add Persona
          </Button>
        )}
      </Stack>

      <Modal
        opened={!!deleteTarget}
        onClose={() => setDeleteTarget(null)}
        title="Delete Persona"
        size="sm"
        centered
      >
        <Stack gap="md">
          <Text size="sm">
            Are you sure you want to delete &ldquo;{deleteTarget?.name}&rdquo;? This action cannot be undone.
          </Text>
          <Group justify="flex-end">
            <Button variant="default" size="xs" onClick={() => setDeleteTarget(null)}>
              Cancel
            </Button>
            <Button color="red" size="xs" onClick={confirmDelete}>
              Delete
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Box>
  );
}
