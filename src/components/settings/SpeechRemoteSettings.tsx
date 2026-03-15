import { useCallback, useEffect, useMemo, useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Stack,
  Group,
  Text,
  TextInput,
  PasswordInput,
  Button,
  Badge,
  Combobox,
  InputBase,
  Loader,
  ScrollArea,
  ActionIcon,
  useCombobox,
  NativeSelect,
  Alert,
} from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { RefreshCw, Server, ShieldCheck, ShieldX } from 'lucide-react';
import { addToast, clearToasts } from '../../stores/toasts';
import {
  backendStatusAtom,
  settingsAtom,
  testingRemoteSpeechConnectionAtom,
} from '../../stores/app';
import { remoteSpeechModelsQuery, commands } from '../../lib/commands';
import { invoke } from '../../lib/tauri';
import type { Settings, SettingsPatch, SpeechRemotePreset } from '../../types';

const PRESET_OPTIONS: { value: SpeechRemotePreset; label: string; hint: string }[] = [
  {
    value: 'speaches',
    label: 'Speaches',
    hint: 'Self-hosted OpenAI-compatible transcription server.',
  },
  {
    value: 'openai',
    label: 'OpenAI',
    hint: 'Hosted OpenAI speech-to-text endpoint.',
  },
  {
    value: 'custom',
    label: 'Custom',
    hint: 'Any OpenAI-compatible transcription API.',
  },
];

export function SpeechRemoteSettings() {
  const settings = useAtomValue(settingsAtom);
  const backendStatus = useAtomValue(backendStatusAtom);
  const testingConnection = useAtomValue(testingRemoteSpeechConnectionAtom);
  const setSettings = useSetAtom(settingsAtom);
  const setTestingConnection = useSetAtom(testingRemoteSpeechConnectionAtom);
  const queryClient = useQueryClient();
  const combobox = useCombobox({
    onDropdownClose: () => combobox.resetSelectedOption(),
  });

  const [baseUrlInput, setBaseUrlInput] = useState(settings?.speech_remote_base_url ?? '');
  const [apiKeyInput, setApiKeyInput] = useState('');
  const [modelSearch, setModelSearch] = useState('');
  const [modelInputFocused, setModelInputFocused] = useState(false);

  useEffect(() => {
    if (settings?.speech_remote_base_url != null) {
      setBaseUrlInput(settings.speech_remote_base_url);
    }
  }, [settings?.speech_remote_base_url]);

  useEffect(() => {
    if (
      settings?.speech_provider === 'remote_openai_compatible' &&
      settings.speech_remote_base_url.trim()
    ) {
      queryClient.invalidateQueries({ queryKey: remoteSpeechModelsQuery.queryKey });
    }
  }, [
    queryClient,
    settings?.speech_provider,
    settings?.speech_remote_base_url,
    settings?.speech_remote_preset,
    settings?.speech_remote_api_key_configured,
  ]);

  const hasBaseUrl = !!baseUrlInput.trim();
  const { data: remoteModels = [], isLoading: fetchingModels } = useQuery({
    ...remoteSpeechModelsQuery,
    enabled:
      settings?.speech_provider === 'remote_openai_compatible' &&
      hasBaseUrl,
  });

  const filteredModels = useMemo(() => {
    const query = modelSearch.toLowerCase().trim();
    if (!query) return remoteModels;
    return remoteModels.filter(
      (model) =>
        model.id.toLowerCase().includes(query) ||
        (model.name && model.name.toLowerCase().includes(query)),
    );
  }, [modelSearch, remoteModels]);
  const selectedPreset = PRESET_OPTIONS.find(
    (option) => option.value === settings?.speech_remote_preset,
  );

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

  const refreshModels = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: remoteSpeechModelsQuery.queryKey });
  }, [queryClient]);

  async function testConnection() {
    if (testingConnection) return;
    setTestingConnection(true);
    addToast('info', 'Testing remote speech connection…', undefined, 0);
    try {
      const preview = await commands.testRemoteSpeechConnection();
      clearToasts();
      addToast('success', preview);
      refreshModels();
    } catch (error) {
      clearToasts();
      addToast('error', `Remote speech connection failed: ${String(error)}`);
    } finally {
      setTestingConnection(false);
    }
  }

  function selectModel(value: string) {
    applyPatch({ speech_remote_model: value }, 'Remote speech model updated.');
    setModelSearch('');
    combobox.closeDropdown();
  }

  const options = filteredModels.map((model) => (
    <Combobox.Option value={model.id} key={model.id}>
      <Stack gap={0}>
        <Text size="sm" truncate>
          {model.name && model.name !== model.id ? model.name : model.id}
        </Text>
        {model.name && model.name !== model.id && (
          <Text size="xs" c="dimmed" truncate>{model.id}</Text>
        )}
      </Stack>
    </Combobox.Option>
  ));

  return (
    <Stack gap="md">
      <Group justify="space-between" align="flex-start">
        <Stack gap={4}>
          <Text size="sm" fw={500}>Remote Speech Provider</Text>
          <Text size="xs" c="dimmed">
            ButterVoice will upload recorded audio to the configured transcription API.
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

      <NativeSelect
        label="Preset"
        size="sm"
        data={PRESET_OPTIONS.map((option) => ({
          value: option.value,
          label: option.label,
        }))}
        value={settings.speech_remote_preset}
        onChange={(event) => {
          const nextPreset = event.currentTarget.value as SpeechRemotePreset;
          const descriptor = PRESET_OPTIONS.find((option) => option.value === nextPreset);
          applyPatch(
            { speech_remote_preset: nextPreset },
            `${descriptor?.label ?? 'Remote preset'} selected.`,
          );
        }}
      />
      <Text size="xs" c="dimmed">
        {selectedPreset?.hint}
      </Text>

      <TextInput
        label="Base URL"
        size="sm"
        value={baseUrlInput}
        placeholder="https://your-transcription-service.example/v1"
        onChange={(event) => setBaseUrlInput(event.currentTarget.value)}
        onBlur={(event) => {
          if (event.currentTarget.value !== settings.speech_remote_base_url) {
            applyPatch(
              { speech_remote_base_url: event.currentTarget.value },
              'Remote speech base URL updated.',
            );
          }
        }}
      />

      <PasswordInput
        label="API Key"
        size="sm"
        value={apiKeyInput}
        placeholder={
          settings.speech_remote_api_key_configured
            ? 'Configured in Keychain'
            : 'Optional bearer token'
        }
        onChange={(event) => setApiKeyInput(event.currentTarget.value)}
        onBlur={(event) => {
          if (event.currentTarget.value.trim() || settings.speech_remote_api_key_configured) {
            applyPatch(
              { speech_remote_api_key: event.currentTarget.value },
              event.currentTarget.value.trim()
                ? 'Remote speech API key updated.'
                : 'Remote speech API key cleared.',
            );
            setApiKeyInput('');
          }
        }}
      />
      <Text size="xs" c="dimmed">
        API key configured: {settings.speech_remote_api_key_configured ? 'Yes' : 'No'}
      </Text>

      <Combobox store={combobox} onOptionSubmit={selectModel}>
        <Combobox.Target>
          <InputBase
            label="Remote Model"
            size="sm"
            rightSection={
              fetchingModels ? (
                <Loader size={14} />
              ) : (
                <ActionIcon
                  variant="subtle"
                  size="sm"
                  onClick={(event) => {
                    event.stopPropagation();
                    refreshModels();
                  }}
                  aria-label="Refresh remote model list"
                >
                  <RefreshCw size={14} />
                </ActionIcon>
              )
            }
            value={modelInputFocused ? modelSearch : settings.speech_remote_model}
            placeholder="e.g. Systran/faster-whisper-small.en"
            onChange={(event) => {
              setModelSearch(event.currentTarget.value);
              combobox.openDropdown();
              combobox.updateSelectedOptionIndex();
            }}
            onClick={() => combobox.openDropdown()}
            onFocus={() => {
              setModelInputFocused(true);
              setModelSearch('');
              combobox.openDropdown();
            }}
            onBlur={() => {
              setModelInputFocused(false);
              combobox.closeDropdown();
              const trimmed = modelSearch.trim();
              if (trimmed && trimmed !== settings.speech_remote_model) {
                selectModel(trimmed);
              }
            }}
          />
        </Combobox.Target>
        <Combobox.Dropdown>
          <Combobox.Options>
            <ScrollArea.Autosize mah={220} type="scroll">
              {options.length > 0 ? options : (
                <Combobox.Empty>
                  {hasBaseUrl
                    ? 'No remote models returned. You can still enter one manually.'
                    : 'Enter a base URL first.'}
                </Combobox.Empty>
              )}
            </ScrollArea.Autosize>
          </Combobox.Options>
        </Combobox.Dropdown>
      </Combobox>

      <Group justify="space-between">
        <Button
          variant="default"
          leftSection={<Server size={14} />}
          onClick={testConnection}
          loading={testingConnection}
        >
          Test Connection
        </Button>
        <Button
          variant="subtle"
          onClick={refreshModels}
          disabled={!hasBaseUrl}
        >
          Refresh Models
        </Button>
      </Group>

      <Alert color="blue" variant="light">
        <Stack gap={4}>
          <Text size="sm" fw={500}>Remote model management stays server-side</Text>
          <Text size="xs">
            ButterVoice can discover and select remote models, but installs and admin tasks stay in Speaches or your hosted provider.
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
