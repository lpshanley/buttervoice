import { useCallback, useMemo, useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { Copy, Timer, Activity, Podcast, AlertTriangle, Eye, EyeOff, Hash, Mic, Gauge, Sparkles, Wand2, ShieldAlert } from 'lucide-react';
import {
  Stack, Group, Text, Badge, Center, Kbd, ActionIcon, Tooltip,
  Paper, Box, Alert, Anchor, SimpleGrid, SegmentedControl, Select,
} from '@mantine/core';
import { SectionCard } from '../ui/SectionCard';
import { AudioPlayButton } from '../ui/AudioPlayButton';
import { usageStatsQuery, commands } from '../../lib/commands';
import { useTauriEvent } from '../../lib/hooks/useTauriEvent';
import {
  settingsAtom,
  setupCompleteAtom,
  hasAllPermissionsAtom,
  hasModelDownloadedAtom,
  showLivePipelineAtom,
} from '../../stores/app';
import { latestInfoOperationAtom } from '../../stores/logs';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { RoundtripChart } from './RoundtripChart';
import { ProcessingStepper } from './ProcessingStepper';
import { Switch } from '../ui/Switch';
import { fmtTime, shortId } from '../../lib/format';
import type { TranscriptLogEntry, RoundtripSegment, Settings, SettingsPatch, OutputDestination, Persona } from '../../types';
import { hotkeyDisplayLabel } from '../../types';

async function copyText(text: string, label: string) {
  try {
    await navigator.clipboard.writeText(text);
    addToast('success', `Copied ${label}.`);
  } catch {
    addToast('error', `Failed to copy ${label}.`);
  }
}

function isErrorLog(entry: TranscriptLogEntry) {
  return entry.text.startsWith('[error]');
}

function transcriptStageLabel(entry: TranscriptLogEntry) {
  if (!entry.is_final) return 'Raw Draft';
  if (entry.cleanup_applied) return 'Final (LLM Cleaned)';
  if (entry.cleanup_requested) return 'Final (No Changes)';
  return 'Final';
}

function statusColor(entry: TranscriptLogEntry) {
  if (isErrorLog(entry)) return 'red';
  if (!entry.is_final) return 'blue';
  if (entry.cleanup_applied) return 'teal';
  return 'green';
}

/* ─────────────────────────────────────────────────────────────────────────────
 * Today Stats
 * ───────────────────────────────────────────────────────────────────────── */

const ACCENT = {
  blue:   'rgba(59, 130, 246, 0.85)',
  violet: 'rgba(139, 92, 246, 0.85)',
  amber:  'rgba(245, 158, 11, 0.85)',
  teal:   'rgba(20, 184, 166, 0.85)',
} as const;

function fmtRecTime(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

function TodayStats() {
  const queryClient = useQueryClient();
  const { data: allStats = [] } = useQuery(usageStatsQuery);

  const onTranscriptAdded = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: ['usage-stats'] });
  }, [queryClient]);
  useTauriEvent('transcript-log-added', onTranscriptAdded);

  const today = useMemo(() => {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }, []);

  const stat = useMemo(() => allStats.find((s) => s.date === today), [allStats, today]);

  if (!stat) return null;

  const avgWpm = stat.recording_seconds > 0
    ? Math.round(stat.word_count / (stat.recording_seconds / 60))
    : 0;

  return (
    <Paper p="sm" radius="md" withBorder shadow="xs">
    <SimpleGrid cols={4} spacing={0}>
      <Stack gap={6} align="center" py="xs">
        <Hash size={18} style={{ color: ACCENT.blue, opacity: 0.9, marginBottom: 2 }} />
        <Text size="md" fw={700} lh={1} style={{ fontVariantNumeric: 'tabular-nums' }}>
          {stat.word_count.toLocaleString()}
        </Text>
        <Text size="xs" c="dimmed" fw={500}>Words</Text>
      </Stack>
      <Stack gap={6} align="center" py="xs">
        <Mic size={18} style={{ color: ACCENT.violet, opacity: 0.9, marginBottom: 2 }} />
        <Text size="md" fw={700} lh={1} style={{ fontVariantNumeric: 'tabular-nums' }}>
          {stat.dictation_count.toLocaleString()}
        </Text>
        <Text size="xs" c="dimmed" fw={500}>Dictations</Text>
      </Stack>
      <Stack gap={6} align="center" py="xs">
        <Gauge size={18} style={{ color: ACCENT.amber, opacity: 0.9, marginBottom: 2 }} />
        <Text size="md" fw={700} lh={1} style={{ fontVariantNumeric: 'tabular-nums' }}>
          {avgWpm > 0 ? avgWpm : '—'}
        </Text>
        <Text size="xs" c="dimmed" fw={500}>WPM</Text>
      </Stack>
      <Stack gap={6} align="center" py="xs">
        <Timer size={18} style={{ color: ACCENT.teal, opacity: 0.9, marginBottom: 2 }} />
        <Text size="md" fw={700} lh={1} style={{ fontVariantNumeric: 'tabular-nums' }}>
          {fmtRecTime(stat.recording_seconds)}
        </Text>
        <Text size="xs" c="dimmed" fw={500}>Rec. Time</Text>
      </Stack>
    </SimpleGrid>
    </Paper>
  );
}

/* ─────────────────────────────────────────────────────────────────────────────
 * Empty State
 * ───────────────────────────────────────────────────────────────────────── */

const STUDIO_GREETINGS = [
  'What are you cooking today?',
  'Ready when you are.',
  'Go ahead, say something brilliant.',
  'Your words, your way.',
  'Mic check, one two.',
  'Speak your mind.',
  'The floor is yours.',
  'Say the word.',
  'Dictate your destiny.',
  'Talk it out.',
];

function pickGreeting(): string {
  const today = new Date();
  const seed = today.getFullYear() * 10000 + (today.getMonth() + 1) * 100 + today.getDate();
  return STUDIO_GREETINGS[seed % STUDIO_GREETINGS.length];
}

function EmptyState() {
  const settings = useAtomValue(settingsAtom);
  const setupComplete = useAtomValue(setupCompleteAtom);
  const hasAllPerms = useAtomValue(hasAllPermissionsAtom);
  const hasModel = useAtomValue(hasModelDownloadedAtom);
  const greeting = useMemo(() => pickGreeting(), []);

  return (
    <Center style={{ minHeight: 'calc(100dvh - 220px)' }}>
      <Stack align="center" gap="lg" maw={360}>
        <Box
          style={{
            width: 88,
            height: 88,
            borderRadius: '50%',
            background: 'var(--mantine-primary-color-light)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <Podcast size={38} style={{ color: 'var(--mantine-primary-color-filled)', opacity: 0.45 }} />
        </Box>

        <Stack align="center" gap={4}>
          <Text size="lg" fw={600}>{greeting}</Text>
          {settings && (
            <Text size="sm" c="dimmed" ta="center">
              {settings.dictation_mode === 'toggle' ? (
                <><Kbd size="sm">{hotkeyDisplayLabel(settings.hotkey)}</Kbd> to start &amp; stop</>
              ) : (
                <>Hold <Kbd size="sm">{hotkeyDisplayLabel(settings.hotkey)}</Kbd> to dictate</>
              )}
            </Text>
          )}
        </Stack>

        {!setupComplete && (
          <Alert icon={<AlertTriangle size={16} />} color="orange" variant="light" w="100%">
            <Group gap="xs">
              <Text size="sm">Setup incomplete.</Text>
              {!hasAllPerms ? (
                <Anchor component={Link} to="/settings/permissions" size="sm">Grant permissions</Anchor>
              ) : !hasModel && settings?.speech_provider === 'local_whispercpp' ? (
                <Anchor component={Link} to="/settings/models" size="sm">Download a model</Anchor>
              ) : settings?.speech_provider === 'remote_openai_compatible' ? (
                <Anchor component={Link} to="/settings/models" size="sm">Configure remote speech</Anchor>
              ) : null}
            </Group>
          </Alert>
        )}

        <TodayStats />
      </Stack>
    </Center>
  );
}

/* ─────────────────────────────────────────────────────────────────────────────
 * Operation View
 * ───────────────────────────────────────────────────────────────────────── */

export function InfoView() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);
  const latest = useAtomValue(latestInfoOperationAtom);
  const showLive = useAtomValue(showLivePipelineAtom);
  const setShowLive = useSetAtom(showLivePipelineAtom);
  const [enhancedText, setEnhancedText] = useState<string | null>(null);
  const [enhancing, setEnhancing] = useState(false);
  const [enhanceTraceId, setEnhanceTraceId] = useState<string | null>(null);
  const [enhanceDurationMs, setEnhanceDurationMs] = useState<number>(0);
  const [personaText, setPersonaText] = useState<string | null>(null);
  const [personaTransforming, setPersonaTransforming] = useState(false);
  const [personaTraceId, setPersonaTraceId] = useState<string | null>(null);

  async function togglePostProcessing(enabled: boolean) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch: { post_process_enabled: enabled } as SettingsPatch });
      setSettings(updated);
      addToast('success', `Post-Processing ${enabled ? 'enabled' : 'disabled'}.`);
    } catch (error) {
      addToast('error', `Failed to update setting: ${String(error)}`);
    }
  }

  async function toggleAiEnhancement(enabled: boolean) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch: { llm_cleanup_enabled: enabled } as SettingsPatch });
      setSettings(updated);
      addToast('success', `AI Enhancement ${enabled ? 'enabled' : 'disabled'}.`);
    } catch (error) {
      addToast('error', `Failed to update setting: ${String(error)}`);
    }
  }

  async function changeOutputDestination(value: string) {
    try {
      const dest = value as OutputDestination;
      const updated = await invoke<Settings>('update_settings', { patch: { output_destination: dest } as SettingsPatch });
      setSettings(updated);
    } catch (error) {
      addToast('error', `Failed to update setting: ${String(error)}`);
    }
  }

  async function handleEnhance(text: string) {
    setEnhancing(true);
    const start = performance.now();
    try {
      const result = await commands.enhanceText(text);
      const elapsed = Math.round(performance.now() - start);
      setEnhancedText(result);
      setEnhanceTraceId(latest?.trace_id ?? null);
      setEnhanceDurationMs(elapsed);
      addToast('success', `Text enhanced in ${(elapsed / 1000).toFixed(1)}s.`);
    } catch (error) {
      addToast('error', `Enhancement failed: ${String(error)}`);
    } finally {
      setEnhancing(false);
    }
  }

  async function handlePersonaTransform(text: string, personaId: string) {
    setPersonaTransforming(true);
    try {
      const result = await invoke<string>('transform_with_persona', { text, personaId });
      setPersonaText(result);
      setPersonaTraceId(latest?.trace_id ?? null);
      addToast('success', 'Persona transform complete.');
    } catch (error) {
      addToast('error', `Persona transform failed: ${String(error)}`);
    } finally {
      setPersonaTransforming(false);
    }
  }

  if (!latest) return <EmptyState />;

  const ppEnabled = settings?.post_process_enabled ?? false;
  const aiFeatureEnabled = settings?.beta_ai_enhancement_enabled ?? false;
  const classificationFeatureEnabled =
    settings?.beta_content_classification_enabled ?? false;
  const personasFeatureEnabled = settings?.beta_personas_enabled ?? false;
  const aiEnabled = aiFeatureEnabled && (settings?.llm_cleanup_enabled ?? false);
  const outputDest = settings?.output_destination ?? 'input';
  const baseText = latest.text || (latest.post_process_text ?? latest.raw_text ?? '') || '';
  const hasActiveEnhancement = enhancedText != null && enhanceTraceId === latest.trace_id;
  const outputText = hasActiveEnhancement ? enhancedText : baseText;
  const error = isErrorLog(latest);

  const llmConfigured = !!settings?.llm_cleanup_model && !!settings?.llm_cleanup_base_url;
  const showEnhanceButton = aiFeatureEnabled && llmConfigured && !!baseText;

  // Merge elective enhancement duration into the pipeline segments
  const effectiveEnhancementMs = hasActiveEnhancement
    ? enhanceDurationMs
    : latest.cleanup_roundtrip_duration_ms;

  const recordingMs = latest.recording_duration_ms;
  const classificationMs = latest.classification_duration_ms;
  const personaDurationMs = latest.persona_duration_ms;
  const processingMs = Math.max(
    latest.total_waterfall_duration_ms - recordingMs,
    latest.transcription_duration_ms + latest.post_process_duration_ms + effectiveEnhancementMs + classificationMs + personaDurationMs,
  );

  const allSegments: RoundtripSegment[] = [
    { key: 'transcription' as const, label: 'Transcription', ms: latest.transcription_duration_ms, share: 0 },
    { key: 'post_process' as const, label: 'Post-Process', ms: latest.post_process_duration_ms, share: 0 },
    { key: 'enhancement' as const, label: 'Enhancement', ms: effectiveEnhancementMs, share: 0 },
    { key: 'classification' as const, label: 'Classification', ms: classificationMs, share: 0 },
    { key: 'persona' as const, label: 'Persona', ms: personaDurationMs, share: 0 },
  ];
  // Only show segments that have > 0 ms (except always show transcription)
  const segments: RoundtripSegment[] = allSegments
    .filter((s) => s.ms > 0 || s.key === 'transcription')
    .map((s) => ({ ...s, share: processingMs > 0 ? s.ms / processingMs : 0 }));

  return (
    <Stack gap="lg">
      <TodayStats />

      {/* ── Output Hero ── */}
      <Paper
        p="lg"
        radius="md"
        withBorder
        shadow="sm"
        style={{
          borderLeftWidth: 3,
          borderLeftColor: `var(--mantine-color-${hasActiveEnhancement ? 'teal' : statusColor(latest)}-5)`,
        }}
      >
        <Group justify="space-between" mb="md">
          <Group gap="sm">
            <Badge color={hasActiveEnhancement ? 'teal' : statusColor(latest)} variant="light" size="sm">
              {error ? 'Error' : hasActiveEnhancement ? 'Enhanced' : transcriptStageLabel(latest)}
            </Badge>
            <Text size="xs" c="dimmed">{fmtTime(latest.timestamp_ms)}</Text>
          </Group>
          {!error && (
            <Group gap={4}>
              <AudioPlayButton recordingFile={latest.recording_file} size="sm" />
              <Tooltip label="Copy output">
                <ActionIcon
                  variant="subtle"
                  size="sm"
                  disabled={!outputText}
                  onClick={() => copyText(outputText, 'output')}
                  aria-label="Copy output"
                >
                  <Copy size={14} />
                </ActionIcon>
              </Tooltip>
              {showEnhanceButton && (
                <Tooltip label={enhancing ? 'Enhancing…' : 'Enhance with AI'}>
                  <ActionIcon
                    variant="subtle"
                    size="sm"
                    color="teal"
                    loading={enhancing}
                    onClick={() => handleEnhance(baseText)}
                    aria-label="Enhance with AI"
                  >
                    <Sparkles size={14} />
                  </ActionIcon>
                </Tooltip>
              )}
            </Group>
          )}
        </Group>

        <Box style={{ position: 'relative' }}>
          <Text
            size="sm"
            lh={1.7}
            c={error ? 'red' : undefined}
            style={{
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              opacity: enhancing ? 0.35 : 1,
              transition: 'opacity 200ms ease',
            }}
          >
            {outputText || '(empty transcription)'}
          </Text>
          {enhancing && (
            <Group
              gap={6}
              style={{
                position: 'absolute',
                top: '50%',
                left: '50%',
                transform: 'translate(-50%, -50%)',
              }}
            >
              <Sparkles size={14} style={{ color: 'var(--mantine-color-teal-5)', animation: 'buttervoice-enhance-pulse 1.5s ease-in-out infinite' }} />
              <Text size="xs" fw={500} c="teal">Enhancing…</Text>
            </Group>
          )}
        </Box>

        <Text size="xs" c="dimmed" ff="monospace" mt="md">
          {latest.model_id} &middot; {latest.backend ?? 'unknown'} &middot; {shortId(latest.trace_id)}
        </Text>
      </Paper>

      {/* ── Classification Badge ── */}
      {!error && classificationFeatureEnabled && settings?.content_classification_enabled && latest.classification_result && (
        <Paper p="md" radius="md" withBorder>
          <Group gap="sm" mb={latest.classification_result.categories.length > 0 ? 'xs' : 0}>
            <ShieldAlert
              size={16}
              style={{
                color: latest.classification_result.blocked
                  ? 'var(--mantine-color-red-5)'
                  : latest.classification_result.warning
                    ? 'var(--mantine-color-yellow-5)'
                    : 'var(--mantine-color-green-5)',
              }}
            />
            <Badge
              size="sm"
              variant="light"
              color={
                latest.classification_result.blocked ? 'red'
                : latest.classification_result.warning ? 'yellow'
                : 'green'
              }
            >
              {latest.classification_result.blocked
                ? 'Blocked'
                : latest.classification_result.warning
                  ? 'Warning'
                  : 'Clear'}
            </Badge>
            <Text size="xs" c="dimmed" ff="monospace">
              {latest.classification_result.score.toFixed(2)}
            </Text>
          </Group>
          {latest.classification_result.categories.length > 0 && (
            <Group gap={4}>
              {latest.classification_result.categories.map((cat) => (
                <Badge key={cat.tag} size="xs" variant="dot" color={cat.severity === 'high' ? 'red' : cat.severity === 'medium' ? 'yellow' : 'gray'}>
                  {cat.tag}: {cat.severity}
                </Badge>
              ))}
            </Group>
          )}
          {latest.classification_result.blocked && (
            <Alert color="red" variant="light" mt="xs" p="xs">
              <Text size="xs">Auto-injection blocked. You can still copy the text manually.</Text>
            </Alert>
          )}
        </Paper>
      )}

      {/* ── Persona ── */}
      {!error && personasFeatureEnabled && settings?.persona_enabled && (settings?.personas?.length ?? 0) > 0 && (
        <Paper p="md" radius="md" withBorder style={{ borderColor: 'var(--mantine-color-violet-3)' }}>
          <Group gap="sm" mb="sm">
            <Wand2 size={14} style={{ opacity: 0.6 }} />
            <Text size="sm" fw={500}>Persona</Text>
          </Group>
          <Group gap="sm" mb={personaTraceId === latest.trace_id && personaText ? 'sm' : 0}>
            <Select
              size="xs"
              value={settings?.persona_active_id ?? ''}
              onChange={(v) => {
                if (v) {
                  invoke<Settings>('update_settings', {
                    patch: { persona_active_id: v } as SettingsPatch,
                  }).then(setSettings).catch(() => {});
                }
              }}
              data={(settings?.personas ?? []).map((p: Persona) => ({ value: p.id, label: p.name }))}
              style={{ flex: 1, maxWidth: 220 }}
            />
            <Tooltip label={personaTransforming ? 'Transforming…' : 'Transform with persona'}>
              <ActionIcon
                variant="light"
                size="sm"
                color="violet"
                loading={personaTransforming}
                disabled={!baseText || !settings?.persona_active_id}
                onClick={() => handlePersonaTransform(baseText, settings?.persona_active_id ?? '')}
                aria-label="Transform with persona"
              >
                <Wand2 size={14} />
              </ActionIcon>
            </Tooltip>
          </Group>
          {/* Show pipeline persona output or on-demand persona output */}
          {(() => {
            const pText = personaTraceId === latest.trace_id && personaText
              ? personaText
              : latest.persona_text;
            if (!pText) return null;
            return (
              <Paper p="sm" radius="sm" withBorder mt="xs" style={{ borderColor: 'var(--mantine-color-violet-2)' }}>
                <Group justify="space-between" mb={4}>
                  <Badge size="xs" variant="light" color="violet">
                    {settings?.personas?.find((p: Persona) => p.id === (latest.persona_id ?? settings?.persona_active_id))?.name ?? 'Persona'}
                  </Badge>
                  <Tooltip label="Copy persona output">
                    <ActionIcon
                      variant="subtle"
                      size="xs"
                      onClick={() => copyText(pText, 'persona output')}
                    >
                      <Copy size={12} />
                    </ActionIcon>
                  </Tooltip>
                </Group>
                <Text size="sm" lh={1.7} style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                  {pText}
                </Text>
              </Paper>
            );
          })()}
        </Paper>
      )}

      {/* ── Pipeline ── */}
      {!error && (
        <SectionCard icon={Timer} title="Pipeline">
          <Stack gap="md">
            <RoundtripChart recordingMs={recordingMs} processingMs={processingMs} segments={segments} />

            {/* Pipeline toggles */}
            <Group gap="lg" pt="xs" style={{ borderTop: '1px solid var(--mantine-color-default-border)' }}>
              <Group gap={6}>
                <Text size="xs">Post-Processing</Text>
                <Switch
                  checked={ppEnabled}
                  onChange={togglePostProcessing}
                  label="Toggle Post-Processing"
                />
              </Group>
              {aiFeatureEnabled && (
                <Group gap={6}>
                  <Text size="xs">AI Enhancement</Text>
                  <Switch
                    checked={aiEnabled}
                    onChange={toggleAiEnhancement}
                    label="Toggle AI Enhancement"
                  />
                </Group>
              )}
            </Group>

            {/* Output destination */}
            <Group gap={6} pt="xs" style={{ borderTop: '1px solid var(--mantine-color-default-border)' }}>
              <Text size="xs">Output</Text>
              <SegmentedControl
                size="xs"
                value={outputDest}
                onChange={changeOutputDestination}
                data={[
                  { label: 'Type to Input', value: 'input' },
                  { label: 'Clipboard', value: 'clipboard' },
                  { label: 'None', value: 'none' },
                ]}
              />
            </Group>
          </Stack>
        </SectionCard>
      )}

      {/* ── Processing Stages ── */}
      {!error && (
        <SectionCard
          icon={Activity}
          title="Processing Stages"
          headerRight={
            <Tooltip label={showLive ? 'Hide stage details' : 'Show stage details'}>
              <ActionIcon
                variant={showLive ? 'light' : 'subtle'}
                size="xs"
                color={showLive ? 'blue' : 'gray'}
                onClick={() => setShowLive(!showLive)}
                aria-label="Toggle live pipeline"
              >
                {showLive ? <Eye size={13} /> : <EyeOff size={13} />}
              </ActionIcon>
            </Tooltip>
          }
        >
          {showLive ? (
            <ProcessingStepper entry={latest} />
          ) : (
            <Text size="xs" c="dimmed">
              Stage details hidden. Toggle the eye icon to view the full processing breakdown.
            </Text>
          )}
        </SectionCard>
      )}
    </Stack>
  );
}
