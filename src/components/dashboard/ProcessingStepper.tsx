import { useState } from 'react';
import { useAtomValue } from 'jotai';
import {
  Text, Group, ActionIcon, Tooltip, Collapse, Box, Stack, Loader,
} from '@mantine/core';
import {
  Mic, FileText, Sparkles, ArrowRightToLine, Copy,
  ChevronDown, ChevronRight, Check, ShieldAlert, UserPen,
} from 'lucide-react';
import {
  dictationStateAtom,
  settingsAtom,
  showLivePipelineAtom,
  pipelineStepIndexAtom,
} from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { fmtLatency } from '../../lib/format';
import type { TranscriptLogEntry } from '../../types';

async function copyText(text: string, label: string) {
  try {
    await navigator.clipboard.writeText(text);
    addToast('success', `Copied ${label}.`);
  } catch {
    addToast('error', `Failed to copy ${label}.`);
  }
}

/* ── Step indicator circle ── */

function StepCircle({ state, icon: Icon }: {
  state: 'completed' | 'active' | 'pending';
  icon: React.ComponentType<{ size?: number }>;
}) {
  const size = 26;
  const colors = {
    completed: {
      bg: 'var(--mantine-primary-color-filled)',
      fg: '#fff',
    },
    active: {
      bg: 'var(--mantine-primary-color-light)',
      fg: 'var(--mantine-primary-color-filled)',
    },
    pending: {
      bg: 'var(--mantine-color-default-hover)',
      fg: 'var(--mantine-color-dimmed)',
    },
  };

  const c = colors[state];

  return (
    <Box
      style={{
        width: size,
        height: size,
        borderRadius: '50%',
        background: c.bg,
        color: c.fg,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        flexShrink: 0,
      }}
    >
      {state === 'completed' ? (
        <Check size={13} />
      ) : state === 'active' ? (
        <Loader size={13} color="var(--mantine-primary-color-filled)" />
      ) : (
        <Icon size={13} />
      )}
    </Box>
  );
}

/* ── Collapsible text block ── */

function TextBlock({
  label,
  text,
  note,
  defaultOpen,
}: {
  label: string;
  text: string;
  note?: string | null;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen ?? false);
  const hasText = text.length > 0;

  return (
    <Box>
      <Group gap={4} mb={2}>
        <Text size="xs" fw={500} c="dimmed">{label}</Text>
        {hasText && (
          <>
            <Tooltip label={`Copy ${label.toLowerCase()}`}>
              <ActionIcon
                variant="subtle"
                size="xs"
                onClick={() => copyText(text, label.toLowerCase())}
                aria-label={`Copy ${label.toLowerCase()}`}
              >
                <Copy size={12} />
              </ActionIcon>
            </Tooltip>
            <ActionIcon
              variant="subtle"
              size="xs"
              onClick={() => setOpen(!open)}
              aria-label={open ? 'Collapse' : 'Expand'}
            >
              {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            </ActionIcon>
          </>
        )}
      </Group>
      {note && (
        <Text size="xs" c="dimmed" fs="italic">{note}</Text>
      )}
      {hasText && (
        <Collapse in={open}>
          <Text
            size="sm"
            lh={1.6}
            mt={2}
            style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}
          >
            {text}
          </Text>
        </Collapse>
      )}
    </Box>
  );
}

/* ── Step definitions ── */

type StepKey = 'recording' | 'transcription' | 'processing' | 'classification' | 'persona' | 'output';

interface StepDef {
  key: StepKey;
  label: string;
  icon: React.ComponentType<{ size?: number }>;
}

const ALL_STEPS: StepDef[] = [
  { key: 'recording', label: 'Recording', icon: Mic },
  { key: 'transcription', label: 'Transcription', icon: FileText },
  { key: 'processing', label: 'Processing', icon: Sparkles },
  { key: 'classification', label: 'Classification', icon: ShieldAlert },
  { key: 'persona', label: 'Persona', icon: UserPen },
  { key: 'output', label: 'Output', icon: ArrowRightToLine },
];

/* ── Component ── */

interface ProcessingStepperProps {
  entry: TranscriptLogEntry;
}

export function ProcessingStepper({ entry }: ProcessingStepperProps) {
  const settings = useAtomValue(settingsAtom);
  const dictationState = useAtomValue(dictationStateAtom);
  const showLive = useAtomValue(showLivePipelineAtom);
  const liveStepIndex = useAtomValue(pipelineStepIndexAtom);

  const ppEnabled = settings?.post_process_enabled ?? false;
  const aiEnabled =
    (settings?.beta_ai_enhancement_enabled ?? false) &&
    (settings?.llm_cleanup_enabled ?? false);
  const classificationEnabled = settings?.beta_content_classification_enabled ?? false;
  const personasEnabled = settings?.beta_personas_enabled ?? false;

  // Filter steps: only show classification/persona when they have data
  const hasClassification = !!entry.classification_result;
  const hasPersona = !!entry.persona_text;
  const steps = ALL_STEPS.filter((s) => {
    if (s.key === 'classification') return classificationEnabled && hasClassification;
    if (s.key === 'persona') return personasEnabled && hasPersona;
    return true;
  });

  const isProcessing = dictationState !== 'idle' && dictationState !== 'error';
  const activeStep = showLive && isProcessing ? liveStepIndex : steps.length;

  /* Per-stage texts */
  const rawText = (entry.raw_text ?? entry.text) || '';
  const postProcessText = (entry.post_process_text ?? rawText) || '';
  const enhancedText = entry.text || '';
  const outputText = enhancedText || postProcessText || rawText;

  /* Per-stage durations */
  const durations: Record<StepKey, number> = {
    recording: entry.recording_duration_ms,
    transcription: entry.transcription_duration_ms,
    processing: entry.post_process_duration_ms + entry.cleanup_roundtrip_duration_ms,
    classification: entry.classification_duration_ms,
    persona: entry.persona_duration_ms,
    output: 0,
  };

  return (
    <Stack gap={0}>
      {steps.map((stepDef, idx) => {
        const isComplete = idx < activeStep;
        const isActive = idx === activeStep;
        const state = isComplete ? 'completed' : isActive ? 'active' : 'pending';
        const isLast = idx === steps.length - 1;
        const durationMs = durations[stepDef.key];

        return (
          <Box key={stepDef.key}>
            <Group gap="sm" wrap="nowrap" align="stretch">
              {/* Indicator column */}
              <Box style={{ display: 'flex', flexDirection: 'column', alignItems: 'center' }}>
                <StepCircle state={state} icon={stepDef.icon} />
                {!isLast && (
                  <Box
                    style={{
                      width: 2,
                      flex: 1,
                      minHeight: 12,
                      background: isComplete
                        ? 'var(--mantine-primary-color-filled)'
                        : 'var(--mantine-color-default-border)',
                      marginTop: 4,
                      marginBottom: 4,
                      borderRadius: 1,
                    }}
                  />
                )}
              </Box>

              {/* Content column */}
              <Box style={{ flex: 1, paddingBottom: isLast ? 0 : 'var(--mantine-spacing-sm)' }}>
                {/* Step header */}
                <Group gap="xs" wrap="nowrap" style={{ minHeight: 26 }} align="center">
                  <Text size="sm" fw={isComplete || isActive ? 500 : 400} c={state === 'pending' ? 'dimmed' : undefined}>
                    {stepDef.label}
                  </Text>
                  {isComplete && durationMs > 0 && (
                    <Text size="xs" ff="monospace" c="dimmed">
                      {fmtLatency(durationMs)}
                    </Text>
                  )}
                  {isActive && (
                    <Loader size={12} />
                  )}
                </Group>

                {/* Step content — always visible for completed steps */}
                {isComplete && (
                  <Box mt={4}>
                    {stepDef.key === 'recording' && entry.recording_duration_ms > 0 && (
                      <Text size="xs" c="dimmed" fs="italic">
                        Recorded for {(entry.recording_duration_ms / 1000).toFixed(2)}s
                      </Text>
                    )}

                    {stepDef.key === 'transcription' && (
                      <TextBlock
                        label="Raw Transcription"
                        text={rawText}
                        defaultOpen
                      />
                    )}

                    {stepDef.key === 'processing' && (
                      <Stack gap="sm">
                        <TextBlock
                          label="Post-Processed"
                          text={postProcessText}
                          note={
                            !settings?.post_process_enabled
                              ? 'Post-processing was disabled for this operation.'
                              : entry.post_process_edits_applied === 0
                                ? 'Post-processing returned no changes.'
                                : null
                          }
                          defaultOpen={ppEnabled}
                        />
                        <TextBlock
                          label="Enhanced"
                          text={enhancedText}
                          note={
                            !entry.cleanup_requested
                              ? 'AI enhancement was disabled for this operation.'
                              : !entry.cleanup_applied
                                ? 'Enhancement returned no changes.'
                                : null
                          }
                          defaultOpen={aiEnabled}
                        />
                      </Stack>
                    )}

                    {stepDef.key === 'classification' && entry.classification_result && (
                      <Text size="xs" c="dimmed" fs="italic">
                        Score: {entry.classification_result.score.toFixed(2)}
                        {entry.classification_result.blocked && ' — Blocked'}
                        {!entry.classification_result.blocked && entry.classification_result.warning && ' — Warning'}
                        {entry.classification_result.categories.length > 0 && (
                          <> ({entry.classification_result.categories.map((c) => c.tag).join(', ')})</>
                        )}
                      </Text>
                    )}

                    {stepDef.key === 'persona' && entry.persona_text && (
                      <TextBlock
                        label="Persona Output"
                        text={entry.persona_text}
                        defaultOpen
                      />
                    )}

                    {stepDef.key === 'output' && (
                      <TextBlock
                        label="Final Output"
                        text={outputText}
                        defaultOpen
                      />
                    )}
                  </Box>
                )}
              </Box>
            </Group>
          </Box>
        );
      })}
    </Stack>
  );
}
