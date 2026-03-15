import { useCallback, useEffect, useMemo, useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Stack,
  Group,
  Text,
  TextInput,
  PasswordInput,
  Textarea,
  Button,
  Code,
  Collapse,
  ActionIcon,
  Tooltip,
  Combobox,
  InputBase,
  Loader,
  ScrollArea,
  Box,
  useCombobox,
  Alert,
} from '@mantine/core';
import { useDebouncedValue } from '@mantine/hooks';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Eye, EyeOff, RefreshCw, Sparkles, Wand2 } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { ModelOverrideSelect } from '../ui/ModelOverrideSelect';
import { llmModelsQuery } from '../../lib/commands';
import { settingsAtom, testingLlmConnectionAtom } from '../../stores/app';
import { addToast, clearToasts } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings, SettingsPatch } from '../../types';

export function AiEnhancement() {
  const settings = useAtomValue(settingsAtom);
  const testingConnection = useAtomValue(testingLlmConnectionAtom);

  const setSettings = useSetAtom(settingsAtom);
  const setTestingConnection = useSetAtom(testingLlmConnectionAtom);

  const queryClient = useQueryClient();

  const [defaultPrompt, setDefaultPrompt] = useState('');
  const [showDefaultPrompt, setShowDefaultPrompt] = useState(false);

  // Model picker state
  const [modelSearch, setModelSearch] = useState('');
  const [modelInputFocused, setModelInputFocused] = useState(false);

  // Track base URL locally so we can debounce and re-fetch on change
  const [localBaseUrl, setLocalBaseUrl] = useState(settings?.llm_cleanup_base_url ?? '');
  const [debouncedBaseUrl] = useDebouncedValue(localBaseUrl, 500);
  const [apiKeyInput, setApiKeyInput] = useState('');

  const hasBaseUrl = !!debouncedBaseUrl.trim();
  const { data: remoteModels = [], isLoading: fetchingModels } = useQuery({
    ...llmModelsQuery,
    enabled: hasBaseUrl && (settings?.beta_ai_enhancement_enabled ?? false),
  });

  const combobox = useCombobox({
    onDropdownClose: () => combobox.resetSelectedOption(),
  });

  useEffect(() => {
    invoke<string>('get_default_enhancement_prompt').then(setDefaultPrompt).catch(() => {});
  }, []);

  // Sync localBaseUrl when settings load/change externally
  useEffect(() => {
    if (settings?.llm_cleanup_base_url != null) {
      setLocalBaseUrl(settings.llm_cleanup_base_url);
    }
  }, [settings?.llm_cleanup_base_url]);

  // Invalidate cached models when base URL or API key changes
  useEffect(() => {
    if (hasBaseUrl && settings?.beta_ai_enhancement_enabled) {
      queryClient.invalidateQueries({ queryKey: llmModelsQuery.queryKey });
    }
  }, [
    debouncedBaseUrl,
    settings?.beta_ai_enhancement_enabled,
    settings?.llm_cleanup_api_key_configured,
    hasBaseUrl,
    queryClient,
  ]);

  const fetchModels = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: llmModelsQuery.queryKey });
  }, [queryClient]);

  const filteredModels = useMemo(() => {
    const query = modelSearch.toLowerCase().trim();
    if (!query) return remoteModels;
    return remoteModels.filter(
      (m) =>
        m.id.toLowerCase().includes(query) ||
        (m.name && m.name.toLowerCase().includes(query)),
    );
  }, [remoteModels, modelSearch]);

  if (!settings) return null;

  if (!settings.beta_ai_enhancement_enabled) {
    return (
      <Box p="xl" maw={640} mx="auto">
        <Alert color="blue" variant="light">
          AI Enhancement is currently behind a beta flag. Enable it in Advanced &gt; Beta to use this feature.
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

  async function testConnection() {
    if (testingConnection) return;
    setTestingConnection(true);
    addToast('info', 'Testing LLM connection…', undefined, 0);
    try {
      const preview = await invoke<string>('test_llm_cleanup_connection');
      clearToasts();
      addToast('success', `LLM connection OK. Sample response: ${preview}`);
    } catch (error) {
      clearToasts();
      addToast('error', `LLM connection failed: ${String(error)}`);
    } finally {
      setTestingConnection(false);
    }
  }

  function selectModel(value: string) {
    applyPatch({ llm_cleanup_model: value }, 'LLM model updated.');
    setModelSearch('');
    combobox.closeDropdown();
  }

  const options = filteredModels.map((m) => (
    <Combobox.Option value={m.id} key={m.id}>
      {m.name && m.name !== m.id ? (
        <Stack gap={0}>
          <Text size="sm" truncate>{m.name}</Text>
          <Text size="xs" c="dimmed" truncate>{m.id}</Text>
        </Stack>
      ) : (
        <Text size="sm" truncate>{m.id}</Text>
      )}
    </Combobox.Option>
  ));

  // Show "use custom value" option when search doesn't match any option exactly
  const searchTrimmed = modelSearch.trim();
  const exactMatch = searchTrimmed && remoteModels.some((m) => m.id === searchTrimmed);

  return (
    <Box p="xl" maw={640} mx="auto">
      <Stack gap="lg">
        {/* ── Connection ── */}
        <SectionCard icon={Sparkles} title="AI Enhancement">
          <Stack gap="md">
            <Group justify="space-between">
              <Text size="sm" fw={500}>Enable AI Enhancement</Text>
              <Switch
                checked={settings.llm_cleanup_enabled}
                onChange={(v) => applyPatch({ llm_cleanup_enabled: v }, `LLM cleanup ${v ? 'enabled' : 'disabled'}.`)}
                label="Toggle LLM cleanup pass"
              />
            </Group>

            <TextInput
              label="Base URL"
              size="sm"
              value={localBaseUrl}
              placeholder="https://openrouter.ai/api/v1"
              onChange={(e) => setLocalBaseUrl(e.target.value)}
              onBlur={(e) => {
                const val = e.target.value;
                if (val !== settings.llm_cleanup_base_url) {
                  applyPatch({ llm_cleanup_base_url: val }, 'LLM base URL updated.');
                }
              }}
            />

            <Combobox
              store={combobox}
              onOptionSubmit={selectModel}
            >
              <Combobox.Target>
                <InputBase
                  label="Model"
                  size="sm"
                  rightSection={
                    fetchingModels ? (
                      <Loader size={14} />
                    ) : (
                      <Tooltip label="Refresh model list">
                        <ActionIcon
                          variant="subtle"
                          size="sm"
                          onClick={(e) => {
                            e.stopPropagation();
                            fetchModels();
                          }}
                        >
                          <RefreshCw size={14} />
                        </ActionIcon>
                      </Tooltip>
                    )
                  }
                  value={modelInputFocused ? modelSearch : settings.llm_cleanup_model}
                  onChange={(e) => {
                    setModelSearch(e.currentTarget.value);
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
                    if (searchTrimmed && searchTrimmed !== settings.llm_cleanup_model) {
                      selectModel(searchTrimmed);
                    } else {
                      setModelSearch('');
                    }
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && searchTrimmed && !exactMatch) {
                      e.preventDefault();
                      selectModel(searchTrimmed);
                    }
                  }}
                  placeholder={
                    fetchingModels
                      ? 'Loading models…'
                      : 'Search or type a model ID'
                  }
                />
              </Combobox.Target>

              <Combobox.Dropdown>
                <Combobox.Options>
                  <ScrollArea.Autosize mah={280} type="scroll">
                    {fetchingModels ? (
                      <Combobox.Empty>Loading models…</Combobox.Empty>
                    ) : options.length > 0 ? (
                      <>
                        {options}
                        {searchTrimmed && !exactMatch && (
                          <Combobox.Option value={searchTrimmed}>
                            <Text size="sm" fs="italic">Use &ldquo;{searchTrimmed}&rdquo;</Text>
                          </Combobox.Option>
                        )}
                      </>
                    ) : searchTrimmed ? (
                      <Combobox.Option value={searchTrimmed}>
                        <Text size="sm" fs="italic">Use &ldquo;{searchTrimmed}&rdquo;</Text>
                      </Combobox.Option>
                    ) : (
                      <Combobox.Empty>
                        {debouncedBaseUrl.trim()
                          ? 'No models found'
                          : 'Enter a base URL to load models'}
                      </Combobox.Empty>
                    )}
                  </ScrollArea.Autosize>
                </Combobox.Options>
              </Combobox.Dropdown>
            </Combobox>

            <PasswordInput
              label="API Key"
              size="sm"
              value={apiKeyInput}
              placeholder="sk-or-v1-..."
              onChange={(e) => setApiKeyInput(e.currentTarget.value)}
              onBlur={(e) => {
                applyPatch({ llm_cleanup_api_key: e.target.value }, 'LLM API key updated.');
                setApiKeyInput('');
              }}
            />

            <Text size="xs" c="dimmed">
              API key configured: {settings.llm_cleanup_api_key_configured ? 'Yes' : 'No'}
            </Text>

            <ModelOverrideSelect
              value={settings.llm_cleanup_model_override}
              onChange={(val) =>
                applyPatch(
                  { llm_cleanup_model_override: val },
                  val ? 'Enhancement model override set.' : 'Enhancement model override cleared.',
                )
              }
              label="Enhancement Model Override"
              description="Leave empty to use the default model above"
            />

            <Text size="xs" c="dimmed">
              Uses OpenAI-compatible <Code>/chat/completions</Code>. OpenRouter and local providers are supported.
            </Text>

            <Button
              variant="default"
              size="xs"
              onClick={testConnection}
              disabled={testingConnection}
              style={{ alignSelf: 'flex-start' }}
            >
              {testingConnection ? 'Testing…' : 'Test Connection'}
            </Button>
          </Stack>
        </SectionCard>

        {/* ── Enhancement Prompt ── */}
        <SectionCard icon={Wand2} title="Enhancement Prompt">
          <Stack gap="md">
            <Group justify="space-between">
              <Text size="sm" fw={500}>Use Custom Enhancement Prompt</Text>
              <Switch
                checked={settings.llm_cleanup_use_custom_prompt}
                onChange={(v) =>
                  applyPatch(
                    { llm_cleanup_use_custom_prompt: v },
                    `Custom enhancement prompt ${v ? 'enabled' : 'disabled'}.`,
                  )
                }
                label="Override the built-in enhancement prompt"
              />
            </Group>

            <Textarea
              label="Custom Enhancement Prompt"
              size="sm"
              autosize
              minRows={4}
              maxRows={14}
              defaultValue={settings.llm_cleanup_custom_prompt}
              placeholder="Enter a custom enhancement prompt…"
              disabled={!settings.llm_cleanup_use_custom_prompt}
              onBlur={(e) =>
                applyPatch({ llm_cleanup_custom_prompt: e.target.value }, 'Custom enhancement prompt updated.')
              }
            />

            <Group gap="xs">
              <Tooltip label={showDefaultPrompt ? 'Hide default enhancement prompt' : 'View default enhancement prompt'}>
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
                {showDefaultPrompt ? 'Hide' : 'View'} default enhancement prompt
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
                styles={{ input: { opacity: 0.7, fontFamily: 'monospace', fontSize: 'var(--mantine-font-size-xs)' } }}
              />
            </Collapse>
          </Stack>
        </SectionCard>
      </Stack>
    </Box>
  );
}
