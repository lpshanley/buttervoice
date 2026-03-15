import { useMemo, useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Stack, Group, Text, Badge, Button, Box, Center, ActionIcon, Tooltip, Modal,
} from '@mantine/core';
import { Copy, Clock } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { AudioPlayButton } from '../ui/AudioPlayButton';
import { historyLogTailAtom, transcriptLogsAtom } from '../../stores/logs';
import { addToast } from '../../stores/toasts';
import { commands } from '../../lib/commands';
import { fmtTime, fmtDuration, shortId } from '../../lib/format';

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    addToast('success', 'Copied to clipboard.');
  } catch {
    addToast('error', 'Failed to copy text.');
  }
}

export function HistoryLogViewer() {
  const historyLogTail = useAtomValue(historyLogTailAtom);
  const setTranscriptLogs = useSetAtom(transcriptLogsAtom);
  const reversed = useMemo(() => [...historyLogTail].reverse(), [historyLogTail]);
  const [confirmOpen, setConfirmOpen] = useState(false);

  async function clearLogsAndRecordings() {
    try {
      await commands.clearTranscriptLogsAndRecordings();
      setTranscriptLogs([]);
      addToast('success', 'Transcription history and recordings cleared.');
    } catch (error) {
      addToast('error', `Failed clearing logs: ${String(error)}`);
    } finally {
      setConfirmOpen(false);
    }
  }

  return (
    <Box p="xl" maw={720} mx="auto">
      <SectionCard icon={Clock} title="Transcription History">
        <Stack gap={0}>
          <Group justify="space-between" mb="md">
            <Text size="xs" c="dimmed">{historyLogTail.length} entries</Text>
            <Button
              variant="subtle"
              size="compact-xs"
              onClick={() => setConfirmOpen(true)}
              disabled={historyLogTail.length === 0}
            >
              Clear
            </Button>
          </Group>

          <Box style={{ maxHeight: 'calc(100dvh - 240px)', overflowY: 'auto' }}>
            {reversed.map((entry) => (
              <Box
                key={entry.request_id}
                py="xs"
                style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}
              >
                <Group gap="sm" mb={4}>
                  <Text size="xs" c="dimmed">{fmtTime(entry.timestamp_ms)}</Text>
                  <Badge size="xs" color={entry.is_final ? 'green' : 'blue'} variant="light">
                    {entry.text.startsWith('[error]') ? 'Error' : entry.is_final ? 'Final' : 'Draft'}
                  </Badge>
                  <Text size="xs" c="dimmed">
                    {fmtDuration(entry.total_waterfall_duration_ms)} &middot; {entry.model_id} &middot; {shortId(entry.trace_id)}
                  </Text>
                  <AudioPlayButton recordingFile={entry.recording_file} />
                  <Tooltip label="Copy text">
                    <ActionIcon
                      variant="subtle"
                      size="xs"
                      onClick={() => copyText(entry.text)}
                      aria-label="Copy text"
                    >
                      <Copy size={14} />
                    </ActionIcon>
                  </Tooltip>
                </Group>
                <Text size="sm" lh={1.6} style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                  {entry.text}
                </Text>
              </Box>
            ))}
            {reversed.length === 0 && (
              <Center py="xl">
                <Text size="sm" c="dimmed">No transcription history yet.</Text>
              </Center>
            )}
          </Box>
        </Stack>
      </SectionCard>

      <Modal
        opened={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        title="Clear transcription history?"
        size="sm"
        centered
      >
        <Text size="sm" mb="lg">
          This will delete all transcription log entries and their associated audio recordings from
          disk.
        </Text>
        <Group justify="flex-end" gap="sm">
          <Button variant="default" size="xs" onClick={() => setConfirmOpen(false)}>
            Cancel
          </Button>
          <Button color="red" size="xs" onClick={clearLogsAndRecordings}>
            Clear All
          </Button>
        </Group>
      </Modal>
    </Box>
  );
}
