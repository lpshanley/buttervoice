import { useRef, useEffect } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import { Stack, Group, Text, Button, Box, Center } from '@mantine/core';
import { canOpenDebugTabAtom } from '../../stores/app';
import { debugLogTailAtom, debugLogsAtom } from '../../stores/logs';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { fmtTime, shortId } from '../../lib/format';
import { ReliabilityPanel } from './ReliabilityPanel';

export function DebugLogViewer() {
  const canOpenDebug = useAtomValue(canOpenDebugTabAtom);
  const debugLogTail = useAtomValue(debugLogTailAtom);
  const setDebugLogs = useSetAtom(debugLogsAtom);
  const viewportRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (viewportRef.current) {
      viewportRef.current.scrollTop = viewportRef.current.scrollHeight;
    }
  }, [debugLogTail]);

  async function clearLogs() {
    try {
      await invoke('clear_debug_logs');
      setDebugLogs([]);
      addToast('success', 'Debug logs cleared.');
    } catch (error) {
      addToast('error', `Failed clearing debug logs: ${String(error)}`);
    }
  }

  return (
    <Stack gap={0} h="100%">
      <Box p="md" style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}>
        <ReliabilityPanel />
      </Box>
      <Group justify="space-between" px="md" py="xs" style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}>
        <Group gap="sm">
          <Box
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              backgroundColor: canOpenDebug ? 'var(--mantine-color-green-6)' : 'var(--mantine-color-dimmed)',
            }}
          />
          <Text size="xs" c="dimmed">{canOpenDebug ? 'Live' : 'Paused'}</Text>
          <Text size="xs" c="dimmed">{debugLogTail.length} lines</Text>
        </Group>
        <Button variant="subtle" size="compact-xs" onClick={clearLogs}>Clear</Button>
      </Group>

      <Box ref={viewportRef} p="md" ff="monospace" fz="xs" lh="xl" style={{ flex: 1, overflowY: 'auto' }}>
        {debugLogTail.map((entry, i) => (
          <Group key={`${entry.timestamp_ms}-${i}`} gap="sm" py={2} wrap="nowrap">
            <Text size="xs" c="dimmed" style={{ flexShrink: 0, width: 140 }} ff="monospace">{fmtTime(entry.timestamp_ms)}</Text>
            <Text size="xs" c="blue" style={{ flexShrink: 0, width: 60 }} ff="monospace">{shortId(entry.trace_id)}</Text>
            <Text size="xs" c="dimmed" style={{ flexShrink: 0, width: 100 }} truncate ff="monospace">{entry.scope}</Text>
            <Text size="xs" ff="monospace">{entry.message}</Text>
          </Group>
        ))}
        {debugLogTail.length === 0 && (
          <Center py="xl">
            <Text size="sm" c="dimmed">No debug logs yet. Enable debug logging in Settings &gt; Advanced.</Text>
          </Center>
        )}
      </Box>
    </Stack>
  );
}
