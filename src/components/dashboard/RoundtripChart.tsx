import { Box, Divider, Group, Stack, Text } from '@mantine/core';
import { Mic } from 'lucide-react';
import { fmtLatency, fmtMetricDuration } from '../../lib/format';
import type { RoundtripSegment } from '../../types';

const segmentColors: Record<string, string> = {
  transcription: 'var(--mantine-color-orange-5)',
  post_process: 'var(--mantine-color-yellow-5)',
  enhancement: 'var(--mantine-color-blue-5)',
  classification: 'var(--mantine-color-red-5)',
  persona: 'var(--mantine-color-violet-5)',
};

interface RoundtripChartProps {
  recordingMs: number;
  processingMs: number;
  segments: RoundtripSegment[];
}

export function RoundtripChart({ recordingMs, processingMs, segments }: RoundtripChartProps) {
  const maxShare = Math.max(...segments.map((s) => s.share), 0.01);

  return (
    <Stack gap="md">
      {/* ── Recording callout ── */}
      <Group gap="sm" wrap="nowrap">
        <Mic size={13} style={{ opacity: 0.4, flexShrink: 0 }} />
        <Text size="xs" c="dimmed">
          Recorded for{' '}
          <Text span ff="monospace" fw={600} c="var(--mantine-color-text)">
            {(recordingMs / 1000).toFixed(2)}s
          </Text>
        </Text>
      </Group>

      <Divider
        label={
          <Text size="xs" c="dimmed" fw={500}>
            Processing latency
          </Text>
        }
        labelPosition="left"
      />

      {/* ── Processing waterfall ── */}
      {segments.map((segment) => (
        <Group key={segment.key} gap="md" wrap="nowrap">
          <Text size="xs" c="dimmed" w={88} ta="right" style={{ flexShrink: 0 }}>
            {segment.label}
          </Text>
          <Box
            style={{
              flex: 1,
              height: 8,
              borderRadius: 4,
              background: 'rgba(128, 128, 128, 0.08)',
              overflow: 'hidden',
            }}
          >
            <Box
              style={{
                width: segment.share > 0 ? `${(segment.share / maxShare) * 100}%` : 0,
                minWidth: segment.ms > 0 ? 4 : 0,
                height: '100%',
                borderRadius: 4,
                background: segmentColors[segment.key],
                transition: 'width 400ms ease',
              }}
            />
          </Box>
          <Text
            size="xs"
            ff="monospace"
            c="dimmed"
            w={56}
            ta="right"
            style={{ flexShrink: 0 }}
          >
            {fmtLatency(segment.ms)}
          </Text>
          <Text
            size="xs"
            c="dimmed"
            w={36}
            ta="right"
            style={{ flexShrink: 0 }}
          >
            {segment.ms > 0 ? `${(segment.share * 100).toFixed(0)}%` : ''}
          </Text>
        </Group>
      ))}

      <Group justify="flex-end" mt={4}>
        <Text size="xs" fw={600} c="dimmed" ff="monospace">
          {fmtMetricDuration(processingMs)} processing
        </Text>
      </Group>
    </Stack>
  );
}
