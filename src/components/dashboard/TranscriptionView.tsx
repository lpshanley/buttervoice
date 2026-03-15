import { useAtomValue } from 'jotai';
import { Copy, AlertTriangle } from 'lucide-react';
import { Link } from '@tanstack/react-router';
import { Stack, Group, Text, ActionIcon, Alert, Kbd, Anchor, Paper, Tooltip } from '@mantine/core';
import {
  settingsAtom,
  backendStatusAtom,
  selectedModelIdAtom,
  setupCompleteAtom,
  hasAllPermissionsAtom,
  hasModelDownloadedAtom,
} from '../../stores/app';
import { lastTranscriptEntryAtom } from '../../stores/logs';
import { addToast } from '../../stores/toasts';
import { hotkeyDisplayLabel } from '../../types';

async function copyText(text: string, label: string) {
  try {
    await navigator.clipboard.writeText(text);
    addToast('success', `Copied ${label}.`);
  } catch {
    addToast('error', `Failed to copy ${label}.`);
  }
}

export function TranscriptionView() {
  const settings = useAtomValue(settingsAtom);
  const backendStatus = useAtomValue(backendStatusAtom);
  const selectedModelId = useAtomValue(selectedModelIdAtom);
  const setupComplete = useAtomValue(setupCompleteAtom);
  const hasAllPerms = useAtomValue(hasAllPermissionsAtom);
  const hasModel = useAtomValue(hasModelDownloadedAtom);
  const entry = useAtomValue(lastTranscriptEntryAtom);

  const rawText = entry?.raw_text ?? '';
  const cleanedText = entry?.text ?? '';
  const hasEntry = !!entry;

  return (
    <Stack gap="md">
      <Group justify="space-between">
        <Text size="xs" c="dimmed">
          {backendStatus?.backend ?? 'unknown'} &middot;{' '}
          {backendStatus?.effective_compute_mode ?? settings?.compute_mode ?? 'auto'} &middot;{' '}
          {selectedModelId ?? 'Not selected'}
        </Text>
      </Group>

      <Paper p="sm" radius="md" withBorder>
        <Group justify="space-between" mb="xs">
          <Text size="xs" fw={500} c="dimmed">Raw Transcription</Text>
          <Tooltip label="Copy raw transcription">
            <ActionIcon
              variant="subtle"
              size="xs"
              disabled={!rawText}
              onClick={() => copyText(rawText, 'raw transcription')}
              aria-label="Copy raw transcription"
            >
              <Copy size={14} />
            </ActionIcon>
          </Tooltip>
        </Group>
        <Text size="sm" lh={1.6} style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
          {rawText || (hasEntry ? cleanedText : 'No transcriptions yet.')}
        </Text>
      </Paper>

      <Paper p="sm" radius="md" withBorder>
        <Group justify="space-between" mb="xs">
          <Text size="xs" fw={500} c="dimmed">AI Enhanced</Text>
          <Tooltip label="Copy AI enhanced transcription">
            <ActionIcon
              variant="subtle"
              size="xs"
              disabled={!cleanedText}
              onClick={() => copyText(cleanedText, 'AI enhanced transcription')}
              aria-label="Copy AI enhanced transcription"
            >
              <Copy size={14} />
            </ActionIcon>
          </Tooltip>
        </Group>
        <Text size="sm" lh={1.6} style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
          {cleanedText || 'No transcriptions yet.'}
        </Text>
      </Paper>

      {!hasEntry && settings && (
        <Text size="xs" c="dimmed">
          {settings.dictation_mode === 'toggle' ? (
            <>Press <Kbd>{hotkeyDisplayLabel(settings.hotkey)}</Kbd> to start/stop</>
          ) : (
            <>Hold <Kbd>{hotkeyDisplayLabel(settings.hotkey)}</Kbd> to start</>
          )}
        </Text>
      )}

      {!setupComplete && (
        <Alert icon={<AlertTriangle size={16} />} color="orange" variant="light">
          <Group gap="xs">
            <Text size="sm">Setup incomplete.</Text>
            {!hasAllPerms ? (
              <Anchor component={Link} to="/settings/permissions" size="sm">Grant permissions</Anchor>
            ) : !hasModel ? (
              <Anchor component={Link} to="/settings/models" size="sm">Download a model</Anchor>
            ) : null}
          </Group>
        </Alert>
      )}
    </Stack>
  );
}
