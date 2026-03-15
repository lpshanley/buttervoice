import { useEffect, useRef } from 'react';
import { Box, Group, Text } from '@mantine/core';

interface MeterBarProps {
  level: number;
  label?: string;
}

const SEGMENT_COUNT = 44;
const SEGMENT_GAP = 1.5;
const SEGMENT_HEIGHT = 14;
const PEAK_HOLD_FRAMES = 50;
const PEAK_DECAY_SPEED = 0.5;

function getSegmentColor(index: number, active: boolean): string {
  const pct = index / SEGMENT_COUNT;
  if (active) {
    if (pct < 0.55) return '#10b981';
    if (pct < 0.75) return '#22d3ee';
    if (pct < 0.88) return '#f59e0b';
    return '#ef4444';
  }
  if (pct < 0.55) return 'rgba(16, 185, 129, 0.1)';
  if (pct < 0.75) return 'rgba(34, 211, 238, 0.1)';
  if (pct < 0.88) return 'rgba(245, 158, 11, 0.1)';
  return 'rgba(239, 68, 68, 0.1)';
}

export function MeterBar({ level, label }: MeterBarProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animRef = useRef(0);
  const smoothLevelRef = useRef(0);
  const peakRef = useRef(0);
  const peakHoldRef = useRef(0);
  const levelRef = useRef(0);
  const initializedRef = useRef(false);

  levelRef.current = Math.max(0, Math.min(100, level)) / 100;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    function draw() {
      const cvs = canvasRef.current;
      if (!cvs) return;
      const c = cvs.getContext('2d');
      if (!c) return;

      const dpr = window.devicePixelRatio || 1;
      const rect = cvs.getBoundingClientRect();
      const w = rect.width;
      const h = rect.height;

      if (!initializedRef.current || cvs.width !== Math.round(w * dpr) || cvs.height !== Math.round(h * dpr)) {
        cvs.width = Math.round(w * dpr);
        cvs.height = Math.round(h * dpr);
        c.scale(dpr, dpr);
        initializedRef.current = true;
      }

      // Smooth level: fast attack, slower release
      const target = levelRef.current;
      const current = smoothLevelRef.current;
      if (target > current) {
        smoothLevelRef.current += (target - current) * 0.4;
      } else {
        smoothLevelRef.current += (target - current) * 0.08;
      }

      // Peak hold with decay
      if (smoothLevelRef.current > peakRef.current) {
        peakRef.current = smoothLevelRef.current;
        peakHoldRef.current = PEAK_HOLD_FRAMES;
      } else if (peakHoldRef.current > 0) {
        peakHoldRef.current--;
      } else {
        peakRef.current = Math.max(
          peakRef.current - PEAK_DECAY_SPEED / SEGMENT_COUNT,
          smoothLevelRef.current,
        );
      }

      c.clearRect(0, 0, w, h);

      const totalGaps = (SEGMENT_COUNT - 1) * SEGMENT_GAP;
      const segW = (w - totalGaps) / SEGMENT_COUNT;
      const activeCount = Math.round(smoothLevelRef.current * SEGMENT_COUNT);
      const peakIndex = Math.min(
        Math.round(peakRef.current * SEGMENT_COUNT) - 1,
        SEGMENT_COUNT - 1,
      );
      const segH = Math.min(h - 2, SEGMENT_HEIGHT);
      const y = (h - segH) / 2;

      for (let i = 0; i < SEGMENT_COUNT; i++) {
        const x = i * (segW + SEGMENT_GAP);
        const isActive = i < activeCount;
        const isPeak = i === peakIndex && peakIndex >= activeCount && peakRef.current > 0.01;
        const radius = Math.min(segW * 0.28, 1.5);

        if (isPeak) {
          const pct = i / SEGMENT_COUNT;
          if (pct < 0.55) c.fillStyle = 'rgba(16, 185, 129, 0.7)';
          else if (pct < 0.75) c.fillStyle = 'rgba(34, 211, 238, 0.7)';
          else if (pct < 0.88) c.fillStyle = 'rgba(245, 158, 11, 0.7)';
          else c.fillStyle = 'rgba(239, 68, 68, 0.7)';
        } else {
          c.fillStyle = getSegmentColor(i, isActive);
        }

        c.beginPath();
        c.roundRect(x, y, segW, segH, radius);
        c.fill();

        // Subtle glow on active segments near the peak
        if (isActive && i >= activeCount - 3 && activeCount > 2) {
          c.save();
          const pct = i / SEGMENT_COUNT;
          if (pct >= 0.88) {
            c.shadowColor = 'rgba(239, 68, 68, 0.4)';
          } else if (pct >= 0.75) {
            c.shadowColor = 'rgba(245, 158, 11, 0.3)';
          } else {
            c.shadowColor = 'rgba(16, 185, 129, 0.25)';
          }
          c.shadowBlur = 6;
          c.beginPath();
          c.roundRect(x, y, segW, segH, radius);
          c.fill();
          c.restore();
        }
      }

      animRef.current = requestAnimationFrame(draw);
    }

    animRef.current = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(animRef.current);
      initializedRef.current = false;
    };
  }, []);

  const dbValue = level > 0 ? Math.round(20 * Math.log10(level / 100)) : -Infinity;
  const dbDisplay = Number.isFinite(dbValue) ? `${dbValue} dB` : '-\u221E dB';

  return (
    <Box>
      {label && (
        <Group justify="space-between" mb={6}>
          <Text size="xs" fw={500} c="dimmed">{label}</Text>
          <Text
            size="xs"
            ff="monospace"
            c="dimmed"
            style={{ letterSpacing: '-0.02em', minWidth: 52, textAlign: 'right' }}
          >
            {dbDisplay}
          </Text>
        </Group>
      )}
      <Box
        style={{
          background: 'rgba(0, 0, 0, 0.15)',
          borderRadius: 6,
          padding: '3px 4px',
        }}
      >
        <canvas
          ref={canvasRef}
          style={{
            width: '100%',
            height: `${SEGMENT_HEIGHT + 2}px`,
            display: 'block',
          }}
        />
      </Box>
    </Box>
  );
}
