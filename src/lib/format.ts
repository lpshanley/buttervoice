export function fmtTime(timestampMs: number): string {
  return new Date(timestampMs).toLocaleString();
}

export function fmtDuration(ms: number): string {
  return (ms / 1000).toFixed(1) + 's';
}

export function fmtMetricDuration(ms: number): string {
  if (ms <= 0) return '0 ms';
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s (${ms} ms)`;
}

/** Compact latency display: ms when < 1s, seconds when >= 1s */
export function fmtLatency(ms: number): string {
  if (ms <= 0) return '—';
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

export function shortId(value: string | null | undefined): string {
  if (!value) return '--------';
  return value.slice(0, 8);
}
