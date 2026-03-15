import { Box } from '@mantine/core';
import type { DictationState } from '../../types';

const dotColors: Record<DictationState, string> = {
  idle: 'var(--mantine-color-green-6)',
  recording: 'var(--mantine-color-red-6)',
  transcribing: 'var(--mantine-color-orange-6)',
  post_processing: 'var(--mantine-color-yellow-6)',
  injecting: 'var(--mantine-color-blue-6)',
  error: 'var(--mantine-color-red-7)',
};

interface StatusDotProps {
  state: DictationState;
}

export function StatusDot({ state }: StatusDotProps) {
  return (
    <Box
      component="span"
      style={{
        display: 'inline-block',
        width: 9,
        height: 9,
        borderRadius: '50%',
        backgroundColor: dotColors[state],
        boxShadow: state === 'idle' ? `0 0 6px 1px ${dotColors[state]}` : undefined,
        animation: state === 'recording' ? 'pulse-recording 1.2s ease-in-out infinite' : undefined,
        transition: 'background-color 200ms ease',
      }}
    />
  );
}
