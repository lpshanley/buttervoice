import { useCallback, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  ActionIcon, Box, Center, Group, Paper, SegmentedControl, SimpleGrid,
  Stack, Text, Tooltip as MantineTooltip,
} from '@mantine/core';
import {
  ResponsiveContainer, AreaChart, Area, XAxis, YAxis,
  CartesianGrid, Tooltip, type TooltipProps,
} from 'recharts';
import { BarChart3, Mic, Hash, Gauge, Timer, Trash2, TrendingUp } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { commands, usageStatsQuery } from '../../lib/commands';
import { useTauriEvent } from '../../lib/hooks/useTauriEvent';
import type { DailyStat } from '../../types';

/* ── Helpers ── */

type Period = '7d' | '30d' | '90d' | 'all';

function periodDays(period: Period): number | null {
  switch (period) {
    case '7d': return 7;
    case '30d': return 30;
    case '90d': return 90;
    case 'all': return null;
  }
}

function daysAgo(n: number): string {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return isoDate(d);
}

function isoDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function fillGaps(stats: DailyStat[], days: number | null): DailyStat[] {
  if (stats.length === 0) return [];

  const map = new Map(stats.map((s) => [s.date, s]));

  const numDays = days ?? (() => {
    const first = new Date(stats[0].date + 'T00:00:00');
    const last = new Date(stats[stats.length - 1].date + 'T00:00:00');
    return Math.max(Math.ceil((last.getTime() - first.getTime()) / 86400000) + 1, 2);
  })();

  const result: DailyStat[] = [];
  const today = new Date();
  const startDate = new Date(today.getFullYear(), today.getMonth(), today.getDate() - numDays + 1);

  for (let i = 0; i < numDays; i++) {
    const d = new Date(startDate);
    d.setDate(d.getDate() + i);
    const key = isoDate(d);
    result.push(map.get(key) ?? { date: key, word_count: 0, dictation_count: 0, recording_seconds: 0 });
  }

  return result;
}

function fmtRecordingTime(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

function fmtNumber(n: number): string {
  return n.toLocaleString();
}

function fmtDateTick(val: string): string {
  const parts = val.split('-');
  return parts.length >= 3 ? `${parts[1]}/${parts[2]}` : val;
}

/* ── Stat Card ── */

const ACCENT_COLORS = {
  blue:   'rgba(59, 130, 246, 0.85)',
  violet: 'rgba(139, 92, 246, 0.85)',
  amber:  'rgba(245, 158, 11, 0.85)',
  teal:   'rgba(20, 184, 166, 0.85)',
} as const;

interface StatCardProps {
  icon: React.ComponentType<{ size?: number; style?: React.CSSProperties }>;
  label: string;
  value: string;
  description?: string;
  accent: keyof typeof ACCENT_COLORS;
}

function StatCard({ icon: Icon, label, value, description, accent }: StatCardProps) {
  const color = ACCENT_COLORS[accent];
  return (
    <Stack gap={6} align="center" py="sm">
      <Icon size={24} style={{ color, opacity: 0.9, marginBottom: 6 }} />
      <Text size="xl" fw={700} lh={1} ta="center" style={{ fontVariantNumeric: 'tabular-nums' }}>
        {value}
      </Text>
      <Text size="xs" c="dimmed" fw={500} ta="center" style={{ letterSpacing: '0.02em' }}>
        {label}
      </Text>
      {description && (
        <Text size="xs" c="dimmed" ta="center" mt={-4}>{description}</Text>
      )}
    </Stack>
  );
}

/* ── Custom Chart Tooltip ── */

function ChartTooltipContent({ active, payload }: TooltipProps<number, string>) {
  if (!active || !payload?.length) return null;
  const item = payload[0].payload as DailyStat;

  // Pretty date: "Mar 4, 2026"
  const d = new Date(item.date + 'T00:00:00');
  const prettyDate = d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });

  return (
    <Paper
      px="sm"
      py="xs"
      radius="md"
      shadow="md"
      withBorder
      style={{
        backdropFilter: 'blur(12px)',
        background: 'var(--mantine-color-body)',
        border: '1px solid var(--mantine-color-default-border)',
      }}
    >
      <Text size="xs" c="dimmed" fw={500}>{prettyDate}</Text>
      <Group gap="lg" mt={4}>
        <Box>
          <Text size="lg" fw={700} style={{ color: 'rgba(59, 130, 246, 0.9)', fontVariantNumeric: 'tabular-nums' }}>
            {fmtNumber(item.word_count)}
          </Text>
          <Text size="xs" c="dimmed">words</Text>
        </Box>
        <Box>
          <Text size="lg" fw={700} style={{ fontVariantNumeric: 'tabular-nums' }}>
            {item.dictation_count}
          </Text>
          <Text size="xs" c="dimmed">dictations</Text>
        </Box>
      </Group>
    </Paper>
  );
}

/* ── Empty State ── */

function EmptyState() {
  return (
    <Center style={{ minHeight: 'calc(100dvh - 280px)' }}>
      <Stack align="center" gap="lg" maw={320}>
        <Box
          style={{
            width: 72,
            height: 72,
            borderRadius: 16,
            background: 'linear-gradient(135deg, rgba(59, 130, 246, 0.08), rgba(139, 92, 246, 0.08))',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <BarChart3 size={28} style={{ opacity: 0.35 }} />
        </Box>
        <Stack align="center" gap={4}>
          <Text size="lg" fw={600}>No usage data yet</Text>
          <Text size="sm" c="dimmed" ta="center" lh={1.5}>
            Start dictating to see your stats here. Data is tracked automatically with each transcription.
          </Text>
        </Stack>
      </Stack>
    </Center>
  );
}

/* ── Dashboard ── */

export function StatsDashboard() {
  const queryClient = useQueryClient();
  const { data: rawStats = [] } = useQuery(usageStatsQuery);
  const [period, setPeriod] = useState<Period>('7d');

  const onTranscriptAdded = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: ['usage-stats'] });
  }, [queryClient]);
  useTauriEvent('transcript-log-added', onTranscriptAdded);

  const handleClear = useCallback(async () => {
    await commands.clearUsageStats();
    queryClient.invalidateQueries({ queryKey: ['usage-stats'] });
  }, [queryClient]);

  const filtered = useMemo(() => {
    const days = periodDays(period);
    if (!days) return rawStats;
    const cutoff = daysAgo(days);
    return rawStats.filter((s) => s.date >= cutoff);
  }, [rawStats, period]);

  const chartData = useMemo(() => fillGaps(filtered, periodDays(period)), [filtered, period]);

  const totals = useMemo(() => {
    const totalWords = filtered.reduce((sum, s) => sum + s.word_count, 0);
    const totalDictations = filtered.reduce((sum, s) => sum + s.dictation_count, 0);
    const totalRecordingSeconds = filtered.reduce((sum, s) => sum + s.recording_seconds, 0);
    const avgWpm = totalRecordingSeconds > 0
      ? Math.round(totalWords / (totalRecordingSeconds / 60))
      : 0;
    return { totalWords, totalDictations, totalRecordingSeconds, avgWpm };
  }, [filtered]);

  if (rawStats.length === 0) {
    return (
      <Box p="xl" maw={720} mx="auto">
        <EmptyState />
      </Box>
    );
  }

  return (
    <Box p="xl" maw={720} mx="auto">
      <Stack gap="lg">
        {/* Header */}
        <Group justify="space-between" align="center">
          <Group gap={8}>
            <TrendingUp size={15} style={{ opacity: 0.5 }} />
            <Text size="xs" fw={600} tt="uppercase" c="dimmed" style={{ letterSpacing: '0.06em' }}>
              Usage Stats
            </Text>
          </Group>
          <Group gap="xs">
            <SegmentedControl
              size="xs"
              value={period}
              onChange={(v) => setPeriod(v as Period)}
              data={[
                { label: '7d', value: '7d' },
                { label: '30d', value: '30d' },
                { label: '90d', value: '90d' },
                { label: 'All', value: 'all' },
              ]}
            />
            <MantineTooltip label="Clear all stats" position="bottom">
              <ActionIcon variant="subtle" color="gray" size="sm" onClick={handleClear}>
                <Trash2 size={14} />
              </ActionIcon>
            </MantineTooltip>
          </Group>
        </Group>

        {/* Stat cards */}
        <SimpleGrid cols={{ base: 2, sm: 4 }} spacing={0}>
          <StatCard
            icon={Hash}
            label="Words"
            value={fmtNumber(totals.totalWords)}
            accent="blue"
          />
          <StatCard
            icon={Mic}
            label="Dictations"
            value={fmtNumber(totals.totalDictations)}
            accent="violet"
          />
          <StatCard
            icon={Gauge}
            label="Avg WPM"
            value={totals.avgWpm > 0 ? String(totals.avgWpm) : '—'}
            description="words per minute"
            accent="amber"
          />
          <StatCard
            icon={Timer}
            label="Rec. Time"
            value={fmtRecordingTime(totals.totalRecordingSeconds)}
            accent="teal"
          />
        </SimpleGrid>

        {/* Chart */}
        <SectionCard icon={BarChart3} title="Daily Words">
          {chartData.length > 1 ? (
            <Box style={{ width: '100%', height: 220 }}>
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                  <defs>
                    <linearGradient id="wordCountGradient" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="rgba(59, 130, 246, 0.85)" stopOpacity={0.35} />
                      <stop offset="95%" stopColor="rgba(59, 130, 246, 0.85)" stopOpacity={0.02} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid
                    strokeDasharray="3 3"
                    vertical={false}
                    stroke="var(--mantine-color-default-border)"
                    strokeOpacity={0.6}
                  />
                  <XAxis
                    dataKey="date"
                    tickFormatter={fmtDateTick}
                    tick={{ fontSize: 11, fill: 'var(--mantine-color-dimmed)' }}
                    axisLine={false}
                    tickLine={false}
                    minTickGap={16}
                  />
                  <YAxis
                    tick={{ fontSize: 11, fill: 'var(--mantine-color-dimmed)' }}
                    axisLine={false}
                    tickLine={false}
                    allowDecimals={false}
                    width={40}
                  />
                  <Tooltip
                    content={<ChartTooltipContent />}
                    cursor={{ stroke: 'var(--mantine-color-default-border)', strokeDasharray: '4 4' }}
                  />
                  <Area
                    type="monotone"
                    dataKey="word_count"
                    stroke="rgba(59, 130, 246, 0.85)"
                    strokeWidth={2}
                    fill="url(#wordCountGradient)"
                    dot={false}
                    activeDot={{
                      r: 4,
                      fill: '#fff',
                      stroke: 'rgba(59, 130, 246, 0.85)',
                      strokeWidth: 2,
                    }}
                  />
                </AreaChart>
              </ResponsiveContainer>
            </Box>
          ) : (
            <Text size="sm" c="dimmed" ta="center" py="lg">
              Not enough data points to display a chart yet.
            </Text>
          )}
        </SectionCard>
      </Stack>
    </Box>
  );
}
