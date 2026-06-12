import { useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Stack,
  Group,
  Text,
  PasswordInput,
  Button,
  Badge,
  Anchor,
  Alert,
} from '@mantine/core';
import { Server, ShieldCheck, ShieldX } from 'lucide-react';
import { addToast, clearToasts } from '../../stores/toasts';
import {
  backendStatusAtom,
  settingsAtom,
  testingRemoteSpeechConnectionAtom,
} from '../../stores/app';
import { commands } from '../../lib/commands';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings, SettingsPatch } from '../../types';

export function GrokSpeechSettings() {
  const settings = useAtomValue(settingsAtom);
  const backendStatus = useAtomValue(backendStatusAtom);
  const testingConnection = useAtomValue(testingRemoteSpeechConnectionAtom);
  const setSettings = useSetAtom(settingsAtom);
  const setTestingConnection = useSetAtom(testingRemoteSpeechConnectionAtom);

  const [apiKeyInput, setApiKeyInput] = useState('');

  if (!settings) return null;

  async function applyPatch(patch: SettingsPatch, success: string) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch });
      setSettings(updated);
      addToast('success', success);
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  async function testConnection() {
    if (testingConnection) return;
    setTestingConnection(true);
    addToast('info', 'Testing Grok speech connection…', undefined, 0);
    try {
      const preview = await commands.testRemoteSpeechConnection();
      clearToasts();
      addToast('success', preview);
    } catch (error) {
      clearToasts();
      addToast('error', `Grok speech connection failed: ${String(error)}`);
    } finally {
      setTestingConnection(false);
    }
  }

  return (
    <Stack gap="md">
      <Group justify="space-between" align="flex-start">
        <Stack gap={4}>
          <Text size="sm" fw={500}>Grok Speech-to-Text</Text>
          <Text size="xs" c="dimmed">
            ButterVoice will upload recorded audio to xAI&apos;s hosted transcription API.
          </Text>
        </Stack>
        <Badge
          color={backendStatus?.provider_ok ? 'green' : 'yellow'}
          variant="light"
          leftSection={
            backendStatus?.provider_ok ? <ShieldCheck size={12} /> : <ShieldX size={12} />
          }
        >
          {backendStatus?.provider_ok ? 'Configured' : 'Needs setup'}
        </Badge>
      </Group>

      <PasswordInput
        label="API Key"
        size="sm"
        value={apiKeyInput}
        placeholder={
          settings.grok_api_key_configured
            ? 'Configured in Keychain'
            : 'xai-…'
        }
        onChange={(event) => setApiKeyInput(event.currentTarget.value)}
        onBlur={(event) => {
          if (event.currentTarget.value.trim() || settings.grok_api_key_configured) {
            applyPatch(
              { grok_api_key: event.currentTarget.value },
              event.currentTarget.value.trim()
                ? 'Grok API key updated.'
                : 'Grok API key cleared.',
            );
            setApiKeyInput('');
          }
        }}
      />
      <Text size="xs" c="dimmed">
        Create a key at{' '}
        <Anchor href="https://console.x.ai" target="_blank" size="xs">
          console.x.ai
        </Anchor>
        . Transcription is billed by xAI at $0.10 per audio-hour.
      </Text>

      <Group justify="space-between" align="center">
        <Stack gap={0}>
          <Text size="sm">Format numbers &amp; currency</Text>
          <Text size="xs" c="dimmed">
            Convert spoken numbers and amounts to written form (requires a concrete language).
          </Text>
        </Stack>
        <Switch
          checked={settings.grok_text_formatting}
          onChange={(v) => applyPatch({ grok_text_formatting: v }, 'Grok text formatting updated.')}
          label="Toggle Grok text formatting"
        />
      </Group>

      <Group justify="space-between" align="center">
        <Stack gap={0}>
          <Text size="sm">Keep filler words</Text>
          <Text size="xs" c="dimmed">
            Include &quot;uh&quot; and &quot;um&quot; in transcripts instead of removing them.
          </Text>
        </Stack>
        <Switch
          checked={settings.grok_filler_words}
          onChange={(v) => applyPatch({ grok_filler_words: v }, 'Grok filler words updated.')}
          label="Toggle Grok filler words"
        />
      </Group>

      <Group>
        <Button
          variant="default"
          leftSection={<Server size={14} />}
          onClick={testConnection}
          loading={testingConnection}
        >
          Test Connection
        </Button>
      </Group>

      <Alert color="blue" variant="light">
        <Stack gap={4}>
          <Text size="sm" fw={500}>Audio leaves this Mac</Text>
          <Text size="xs">
            While Grok is the active provider, every recording is sent to xAI for
            transcription. Custom dictionary terms are also shared to bias recognition.
          </Text>
        </Stack>
      </Alert>

      {backendStatus?.provider_error && (
        <Text size="xs" c="yellow.8">
          {backendStatus.provider_error}
        </Text>
      )}
    </Stack>
  );
}
