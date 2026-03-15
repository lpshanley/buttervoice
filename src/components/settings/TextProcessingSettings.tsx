import { useEffect, useMemo, useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Stack, Group, Text, Textarea, Button, Slider, Code, Paper, Box,
} from '@mantine/core';
import { Wand2, BookOpen, ShieldCheck, FlaskConical } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { Settings, PipelineResult } from '../../types';

export function TextProcessingSettings() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);
  const [testInput, setTestInput] = useState('');
  const [testResult, setTestResult] = useState<PipelineResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [customDictionaryDraft, setCustomDictionaryDraft] = useState('');

  useEffect(() => {
    if (!settings) return;
    setCustomDictionaryDraft(settings.post_process_custom_dictionary.join('\n'));
  }, [settings?.post_process_custom_dictionary]);

  async function updateSetting(patch: Partial<Settings>) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch });
      setSettings(updated);
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  const dictionaryValidation = useMemo(() => {
    const lines = customDictionaryDraft.split('\n');
    const invalidLines: number[] = [];
    const tokens: string[] = [];
    const seen = new Set<string>();

    for (let i = 0; i < lines.length; i += 1) {
      const line = lines[i].trim();
      if (!line) continue;

      const parts = line.split(/\s+/);
      for (const part of parts) {
        const token = part.trim();
        if (!token) continue;
        if (!/^[A-Za-z']+$/.test(token)) {
          invalidLines.push(i + 1);
          continue;
        }
        const normalized = token.toLowerCase();
        if (!seen.has(normalized)) {
          seen.add(normalized);
          tokens.push(token);
        }
      }
    }

    return {
      invalidLines,
      tokens,
    };
  }, [customDictionaryDraft]);

  const savedDictionaryText = settings?.post_process_custom_dictionary.join('\n') ?? '';
  const dictionaryDirty = customDictionaryDraft !== savedDictionaryText;
  const dictionaryHasErrors = dictionaryValidation.invalidLines.length > 0;

  async function saveCustomDictionary() {
    if (dictionaryHasErrors) {
      addToast('error', 'Fix invalid custom dictionary entries before saving.');
      return;
    }
    await updateSetting({ post_process_custom_dictionary: dictionaryValidation.tokens });
    addToast('success', `Custom dictionary saved (${dictionaryValidation.tokens.length} entries).`);
  }

  function resetCustomDictionaryDraft() {
    setCustomDictionaryDraft(savedDictionaryText);
  }

  if (!settings) return null;

  async function runTest() {
    if (!testInput.trim()) return;
    setTesting(true);
    try {
      const result = await invoke<PipelineResult>('test_post_process', { text: testInput });
      setTestResult(result);
    } catch (error) {
      addToast('error', `Pipeline test failed: ${String(error)}`);
    } finally {
      setTesting(false);
    }
  }

  return (
    <Box p="xl" maw={640} mx="auto">
      <Stack gap="lg">
        {/* ── Enable Toggle ── */}
        <SectionCard icon={Wand2} title="Local Post-Processing">
          <Group justify="space-between">
            <div>
              <Text size="sm" fw={500}>Enable Local Post-Processing</Text>
              <Text size="xs" c="dimmed" mt={2}>
                Automatically fix punctuation, spelling, numbers, and grammar locally without
                sending data to external services.
              </Text>
            </div>
            <Switch
              checked={settings.post_process_enabled}
              onChange={(v) => updateSetting({ post_process_enabled: v })}
              label="Toggle post-processing"
            />
          </Group>
        </SectionCard>

        {settings.post_process_enabled && (
          <>
            {/* ── Corrections ── */}
            <SectionCard icon={Wand2} title="Corrections">
              <Stack gap="sm">
                <Group justify="space-between">
                  <Text size="sm">Spell Correction</Text>
                  <Switch
                    checked={settings.post_process_spell_enabled}
                    onChange={(v) => updateSetting({ post_process_spell_enabled: v })}
                    label="Toggle spell correction"
                  />
                </Group>

                <Group justify="space-between">
                  <div>
                    <Text size="sm">Inverse Text Normalization</Text>
                    <Text size="xs" c="dimmed">
                      Convert spoken numbers, currencies, and dates to written form.
                    </Text>
                  </div>
                  <Switch
                    checked={settings.post_process_itn_enabled}
                    onChange={(v) => updateSetting({ post_process_itn_enabled: v })}
                    label="Toggle ITN"
                  />
                </Group>

                <Group justify="space-between">
                  <Text size="sm">Conservative Grammar Rules</Text>
                  <Switch
                    checked={settings.post_process_grammar_rules_enabled}
                    onChange={(v) => updateSetting({ post_process_grammar_rules_enabled: v })}
                    label="Toggle grammar rules"
                  />
                </Group>

                <Group justify="space-between">
                  <div>
                    <Text size="sm">Aggressive Grammar Correction</Text>
                    <Text size="xs" c="dimmed">Experimental. May change meaning.</Text>
                  </div>
                  <Switch
                    checked={settings.post_process_gec_enabled}
                    onChange={(v) => updateSetting({ post_process_gec_enabled: v })}
                    label="Toggle GEC"
                  />
                </Group>
              </Stack>
            </SectionCard>

            {/* ── Custom Dictionary ── */}
            <SectionCard icon={BookOpen} title="Custom Dictionary">
              <Stack gap="sm">
                <Textarea
                  placeholder="Add domain-specific words (one per line)"
                  autosize
                  minRows={3}
                  maxRows={8}
                  value={customDictionaryDraft}
                  onChange={(e) => setCustomDictionaryDraft(e.currentTarget.value)}
                />
                <Text size="xs" c="dimmed">
                  Words added here will not be flagged as misspellings. Use letters and apostrophes
                  only; phrases are tokenized into words.
                </Text>
                <Group gap="sm">
                  <Button
                    size="xs"
                    onClick={saveCustomDictionary}
                    disabled={!dictionaryDirty || dictionaryHasErrors}
                  >
                    Save Dictionary
                  </Button>
                  <Button
                    size="xs"
                    variant="default"
                    onClick={resetCustomDictionaryDraft}
                    disabled={!dictionaryDirty}
                  >
                    Reset
                  </Button>
                  <Text size="xs" c="dimmed">
                    {dictionaryValidation.tokens.length} valid entries
                  </Text>
                </Group>
                {dictionaryHasErrors && (
                  <Text size="xs" c="red">
                    Invalid entries on line(s): {Array.from(new Set(dictionaryValidation.invalidLines)).join(', ')}.
                    Allowed characters: A-Z and apostrophe.
                  </Text>
                )}
              </Stack>
            </SectionCard>

            {/* ── Safety ── */}
            <SectionCard icon={ShieldCheck} title="Safety">
              <Stack gap="md">
                <div>
                  <Group justify="space-between" mb={4}>
                    <Text size="sm" fw={500}>Confidence Threshold</Text>
                    <Text size="xs" ff="monospace" c="dimmed">
                      {settings.post_process_confidence_threshold.toFixed(2)}
                    </Text>
                  </Group>
                  <Slider
                    min={0}
                    max={1}
                    step={0.05}
                    value={settings.post_process_confidence_threshold}
                    onChange={(v) => updateSetting({ post_process_confidence_threshold: v })}
                    label={(v) => v.toFixed(2)}
                  />
                  <Text size="xs" c="dimmed" mt={4}>
                    Only apply corrections with confidence above this threshold.
                  </Text>
                </div>

                <div>
                  <Group justify="space-between" mb={4}>
                    <Text size="sm" fw={500}>Max Edit Ratio</Text>
                    <Text size="xs" ff="monospace" c="dimmed">
                      {settings.post_process_max_edit_ratio.toFixed(2)}
                    </Text>
                  </Group>
                  <Slider
                    min={0}
                    max={1}
                    step={0.05}
                    value={settings.post_process_max_edit_ratio}
                    onChange={(v) => updateSetting({ post_process_max_edit_ratio: v })}
                    label={(v) => v.toFixed(2)}
                  />
                  <Text size="xs" c="dimmed" mt={4}>
                    Reject edits that change more than this ratio of the original text.
                  </Text>
                </div>
              </Stack>
            </SectionCard>

            {/* ── Test Pipeline ── */}
            <SectionCard icon={FlaskConical} title="Test Pipeline">
              <Stack gap="sm">
                <Textarea
                  placeholder="Enter sample text to preview corrections..."
                  autosize
                  minRows={3}
                  maxRows={6}
                  value={testInput}
                  onChange={(e) => setTestInput(e.currentTarget.value)}
                />
                <Button
                  size="xs"
                  variant="default"
                  onClick={runTest}
                  loading={testing}
                  disabled={!testInput.trim()}
                  style={{ alignSelf: 'flex-start' }}
                >
                  Test Pipeline
                </Button>

                {testResult && (
                  <Paper p="sm" radius="md" withBorder>
                    <Stack gap="xs">
                      <Text size="xs" fw={600}>Output:</Text>
                      <Code block>{testResult.output}</Code>
                      <Group gap="lg">
                        <Text size="xs" c="dimmed">
                          {testResult.applied_edits.length} edits applied
                        </Text>
                        <Text size="xs" c="dimmed">
                          {testResult.rejected_edits.length} edits rejected
                        </Text>
                        <Text size="xs" c="dimmed">
                          {testResult.total_duration_ms}ms
                        </Text>
                      </Group>
                      {testResult.applied_edits.length > 0 && (
                        <>
                          <Text size="xs" fw={600} mt="xs">Applied Edits:</Text>
                          {testResult.applied_edits.map((edit, i) => (
                            <Text key={i} size="xs" c="dimmed">
                              [{edit.source}] &quot;{testResult.input.slice(edit.offset, edit.offset + edit.length)}&quot;
                              {' \u2192 '}
                              &quot;{edit.replacement}&quot;
                              {' '}({edit.rule_id}, conf: {edit.confidence.toFixed(2)})
                            </Text>
                          ))}
                        </>
                      )}
                    </Stack>
                  </Paper>
                )}
              </Stack>
            </SectionCard>
          </>
        )}
      </Stack>
    </Box>
  );
}
