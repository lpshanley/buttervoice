import { useEffect, useState } from 'react';
import { Badge, Button, Code, Group, Paper, Stack, Table, Text } from '@mantine/core';
import { commands } from '../../lib/commands';
import { addToast } from '../../stores/toasts';
import type { PipelineMetricsSnapshot } from '../../types';

const EMPTY: PipelineMetricsSnapshot = {
  dictations_started: 0,
  dictations_succeeded: 0,
  dictations_failed: 0,
  pp_runs: 0,
  pp_edits_applied_total: 0,
  pp_edits_rejected_total: 0,
  llm_attempts: 0,
  llm_success: 0,
  llm_fail: 0,
  llm_timeout: 0,
  llm_skipped_circuit_open: 0,
  stage_latency_histograms: {},
  last_100_failures: [],
  llm_circuit_open: false,
  llm_circuit_open_until_ms: null,
};

export function ReliabilityPanel() {
  const [metrics, setMetrics] = useState<PipelineMetricsSnapshot>(EMPTY);
  const [loading, setLoading] = useState(false);
  const [purging, setPurging] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      const next = await commands.getPipelineMetrics();
      setMetrics(next);
    } catch (error) {
      addToast('error', `Failed fetching reliability metrics: ${String(error)}`);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
    const handle = setInterval(() => {
      commands.getPipelineMetrics().then(setMetrics).catch(() => {});
    }, 2000);
    return () => clearInterval(handle);
  }, []);

  async function clearMetrics() {
    try {
      await commands.clearPipelineMetrics();
      await refresh();
      addToast('success', 'Reliability metrics cleared.');
    } catch (error) {
      addToast('error', `Failed clearing reliability metrics: ${String(error)}`);
    }
  }

  async function purgeArtifacts() {
    setPurging(true);
    try {
      await commands.purgeLocalArtifacts();
      addToast('success', 'Local recordings and logs purged.');
    } catch (error) {
      addToast('error', `Failed purging local artifacts: ${String(error)}`);
    } finally {
      setPurging(false);
    }
  }

  const circuitStatus = metrics.llm_circuit_open ? 'OPEN' : 'CLOSED';

  return (
    <Paper withBorder p="md" radius="md">
      <Stack gap="sm">
        <Group justify="space-between">
          <Text size="sm" fw={600}>Reliability Diagnostics</Text>
          <Group gap="xs">
            <Button size="compact-xs" variant="default" onClick={refresh} loading={loading}>
              Refresh
            </Button>
            <Button size="compact-xs" variant="default" onClick={clearMetrics}>
              Clear Metrics
            </Button>
            <Button size="compact-xs" color="red" variant="light" onClick={purgeArtifacts} loading={purging}>
              Purge Local Artifacts
            </Button>
          </Group>
        </Group>

        <Group gap="lg">
          <Text size="xs">Dictations: <Code>{metrics.dictations_started}</Code> started</Text>
          <Text size="xs">Succeeded: <Code>{metrics.dictations_succeeded}</Code></Text>
          <Text size="xs">Failed: <Code>{metrics.dictations_failed}</Code></Text>
          <Badge color={metrics.llm_circuit_open ? 'red' : 'green'} variant="light">
            LLM Circuit {circuitStatus}
          </Badge>
        </Group>

        <Group gap="lg">
          <Text size="xs">LLM attempts: <Code>{metrics.llm_attempts}</Code></Text>
          <Text size="xs">LLM success: <Code>{metrics.llm_success}</Code></Text>
          <Text size="xs">LLM fail: <Code>{metrics.llm_fail}</Code></Text>
          <Text size="xs">Timeouts: <Code>{metrics.llm_timeout}</Code></Text>
          <Text size="xs">Circuit skips: <Code>{metrics.llm_skipped_circuit_open}</Code></Text>
        </Group>

        {metrics.llm_circuit_open_until_ms && (
          <Text size="xs" c="dimmed">
            Circuit open until: {new Date(metrics.llm_circuit_open_until_ms).toLocaleString()}
          </Text>
        )}

        <Table withTableBorder withColumnBorders striped highlightOnHover>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Stage</Table.Th>
              <Table.Th>&lt;=50</Table.Th>
              <Table.Th>&lt;=100</Table.Th>
              <Table.Th>&lt;=250</Table.Th>
              <Table.Th>&lt;=500</Table.Th>
              <Table.Th>&lt;=1000</Table.Th>
              <Table.Th>&gt;1000</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {Object.entries(metrics.stage_latency_histograms).map(([stage, h]) => (
              <Table.Tr key={stage}>
                <Table.Td>{stage}</Table.Td>
                <Table.Td>{h.le_50_ms}</Table.Td>
                <Table.Td>{h.le_100_ms}</Table.Td>
                <Table.Td>{h.le_250_ms}</Table.Td>
                <Table.Td>{h.le_500_ms}</Table.Td>
                <Table.Td>{h.le_1000_ms}</Table.Td>
                <Table.Td>{h.gt_1000_ms}</Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>

        <Text size="xs" fw={600}>Recent failures</Text>
        <Stack gap={2}>
          {metrics.last_100_failures.slice(-8).map((failure, idx) => (
            <Text key={`${failure.timestamp_ms}-${idx}`} size="xs" c="dimmed">
              {new Date(failure.timestamp_ms).toLocaleTimeString()} [{failure.stage}] {failure.error_code}
            </Text>
          ))}
          {metrics.last_100_failures.length === 0 && (
            <Text size="xs" c="dimmed">No failures recorded.</Text>
          )}
        </Stack>
      </Stack>
    </Paper>
  );
}
