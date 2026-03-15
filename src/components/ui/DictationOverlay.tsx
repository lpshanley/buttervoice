import { useEffect, useRef } from 'react';
import { useAtomValue } from 'jotai';
import { Box } from '@mantine/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { dictationStateAtom, inputLevelAtom } from '../../stores/app';
import { tauriAvailable } from '../../lib/tauri';

type OverlayPresentation = 'shell' | 'window';

const OVERLAY_WIDTH = 'min(12rem, calc(100vw - 0.75rem))';
const PROCESSING_STATES = ['transcribing', 'post_processing', 'injecting'] as const;

/* ─────────────────────────────────────────────────────────────────────────────
 * Recording Visualization — "Ember Flow"
 *
 * Three overlapping waveform layers in warm orange tones. Each layer has
 * distinct frequency, phase speed, and opacity, creating a sense of
 * organic depth. The topmost layer gets a subtle highlight stroke along
 * its edge. Soft glow intensifies with input level.
 * ───────────────────────────────────────────────────────────────────────── */

function drawRecording(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  phase: number,
  level: number,
) {
  const midY = h / 2;
  const amp = Math.max(0.08, Math.min(level * 2.4, 1)) * midY * 0.78;

  ctx.clearRect(0, 0, w, h);

  const layers = [
    { freq: [2.2, 4.8], spd: [0.6, 0.85], scale: 0.45, rgb: [255, 175, 90], a: 0.2 },
    { freq: [3.2, 5.5], spd: [1.0, 1.3], scale: 0.7, rgb: [255, 130, 50], a: 0.35 },
    { freq: [3.8, 7.2], spd: [1.25, 0.7], scale: 1.0, rgb: [255, 105, 25], a: 0.52 },
  ];

  let lastTopY: number[] = [];

  for (const layer of layers) {
    const a = amp * layer.scale;
    const [r, g, b] = layer.rgb;

    const topY: number[] = [];
    for (let x = 0; x <= w; x++) {
      const t = x / w;
      const env = Math.sin(t * Math.PI) ** 1.2;
      const wave =
        Math.sin(t * Math.PI * layer.freq[0] + phase * layer.spd[0]) * 0.6 +
        Math.sin(t * Math.PI * layer.freq[1] + phase * layer.spd[1]) * 0.4;
      topY.push(midY - wave * a * env);
    }

    ctx.save();
    ctx.shadowColor = `rgba(${r}, ${g}, ${b}, ${0.1 + level * 0.22})`;
    ctx.shadowBlur = 3 + level * 12;

    // Symmetric filled waveform
    ctx.beginPath();
    ctx.moveTo(0, midY);
    for (let x = 0; x <= w; x++) ctx.lineTo(x, topY[x]);
    ctx.lineTo(w, midY);
    for (let x = w; x >= 0; x--) ctx.lineTo(x, midY + (midY - topY[x]));
    ctx.closePath();

    const grad = ctx.createLinearGradient(0, midY - a, 0, midY + a);
    grad.addColorStop(0, `rgba(${r}, ${g}, ${b}, ${layer.a * 0.5})`);
    grad.addColorStop(0.5, `rgba(${r}, ${g}, ${b}, ${layer.a})`);
    grad.addColorStop(1, `rgba(${r}, ${g}, ${b}, ${layer.a * 0.5})`);
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.restore();

    lastTopY = topY;
  }

  // Highlight stroke on the topmost layer edge
  if (lastTopY.length > 0) {
    ctx.beginPath();
    ctx.moveTo(0, midY);
    for (let x = 0; x <= w; x++) ctx.lineTo(x, lastTopY[x]);
    ctx.strokeStyle = `rgba(255, 200, 130, ${0.12 + level * 0.3})`;
    ctx.lineWidth = 0.9;
    ctx.stroke();
  }
}

/* ─────────────────────────────────────────────────────────────────────────────
 * Processing Visualization — "Cascade Bars"
 *
 * A row of vertical bar segments in cool blue tones. Two luminous sweeps
 * travel across them at different speeds, causing bars to surge in height
 * and brightness as the sweep passes. The wrap-aware distance calculation
 * ensures the sweep loops seamlessly. A gentle breathing oscillation
 * keeps bars alive even between sweeps.
 * ───────────────────────────────────────────────────────────────────────── */

function drawProcessing(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  time: number,
) {
  ctx.clearRect(0, 0, w, h);

  const midY = h / 2;
  const count = 20;
  const gap = 2.5;
  const barW = (w - gap * (count - 1)) / count;
  const maxH = h * 0.56;

  // Two sweeps at different speeds, wrapping smoothly
  const s1 = (time * 0.00042) % 1;
  const s2 = (time * 0.00028 + 0.5) % 1;

  for (let i = 0; i < count; i++) {
    const t = i / (count - 1);
    const x = i * (barW + gap);

    // Wrap-aware distance from each sweep
    const rawD1 = Math.abs(t - s1);
    const rawD2 = Math.abs(t - s2);
    const d1 = Math.min(rawD1, 1 - rawD1);
    const d2 = Math.min(rawD2, 1 - rawD2);
    const sw1 = Math.max(0, 1 - d1 * 5);
    const sw2 = Math.max(0, 1 - d2 * 6);
    const sweep = Math.max(sw1 * sw1, sw2 * sw2 * 0.55);

    // Gentle breathing + sweep-driven surge
    const breath = 0.14 + Math.sin(t * Math.PI * 2.5 + time * 0.002) * 0.05;
    const barH = (breath + sweep * 0.7) * maxH;

    // Interpolate from muted blue to vivid highlight blue
    const r = Math.round(55 + sweep * 85);
    const g = Math.round(115 + sweep * 80);
    const b = Math.round(205 + sweep * 50);
    const alpha = 0.28 + sweep * 0.62;

    const y = midY - barH / 2;
    const radius = Math.min(barW * 0.32, 2.5);

    ctx.save();
    if (sweep > 0.12) {
      ctx.shadowColor = `rgba(80, 150, 255, ${sweep * 0.35})`;
      ctx.shadowBlur = 4 + sweep * 12;
    }
    ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${alpha})`;
    ctx.beginPath();
    ctx.roundRect(x, y, barW, barH, radius);
    ctx.fill();
    ctx.restore();
  }
}

/* ─────────────────────────────────────────────────────────────────────────────
 * Overlay Component
 * ───────────────────────────────────────────────────────────────────────── */

interface DictationOverlayProps {
  presentation?: OverlayPresentation;
}

export function DictationOverlay({ presentation = 'shell' }: DictationOverlayProps) {
  const dictationState = useAtomValue(dictationStateAtom);
  const inputLevel = useAtomValue(inputLevelAtom);
  const isRecording = dictationState === 'recording';
  const isProcessing = PROCESSING_STATES.includes(
    dictationState as (typeof PROCESSING_STATES)[number],
  );
  const visible = isRecording || isProcessing;

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animRef = useRef<number>(0);
  const phaseRef = useRef(0);
  const smoothLevelRef = useRef(0);
  const levelRef = useRef(0);
  const startTimeRef = useRef(0);

  levelRef.current = Math.max(0, Math.min(100, inputLevel)) / 100;

  // HUD window show/hide
  useEffect(() => {
    if (presentation !== 'window' || !tauriAvailable) return;
    const hudWindow = getCurrentWebviewWindow();
    if (visible) {
      hudWindow.show().catch(() => {});
    } else {
      hudWindow.hide().catch(() => {});
    }
  }, [presentation, visible]);

  // Canvas animation loop — unified for both recording and processing
  useEffect(() => {
    if (!visible) {
      smoothLevelRef.current = 0;
      return;
    }
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const w = rect.width;
    const h = rect.height;
    startTimeRef.current = performance.now();

    const draw = () => {
      if (isRecording) {
        smoothLevelRef.current +=
          (levelRef.current - smoothLevelRef.current) * 0.22;
        drawRecording(ctx, w, h, phaseRef.current, smoothLevelRef.current);
        phaseRef.current -= 0.06;
      } else {
        const elapsed = performance.now() - startTimeRef.current;
        drawProcessing(ctx, w, h, elapsed);
      }
      animRef.current = requestAnimationFrame(draw);
    };

    animRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animRef.current);
  }, [visible, isRecording]);

  if (!visible) return null;

  const isWindow = presentation === 'window';

  const wrapperStyle: React.CSSProperties =
    isWindow
      ? {
          position: 'fixed',
          inset: 0,
          display: 'flex',
          alignItems: 'flex-end',
          justifyContent: 'center',
          padding: '0.4rem',
          pointerEvents: 'none',
          background: 'transparent',
        }
      : {
          position: 'fixed',
          left: '50%',
          bottom: '1.25rem',
          transform: 'translateX(-50%)',
          zIndex: 360,
          width: OVERLAY_WIDTH,
          pointerEvents: 'none',
        };

  // Build the multi-layer shadow
  const recordingShadow = isWindow
    ? [
        '0 0 0 1px rgba(255, 125, 40, 0.08)',
        'inset 0 1px 0 rgba(255, 180, 100, 0.06)',
        '0 2px 6px rgba(0, 0, 0, 0.4)',
        '0 6px 24px rgba(255, 120, 30, 0.16)',
        '0 0 48px rgba(255, 100, 20, 0.1)',
      ].join(', ')
    : [
        '0 0 0 1px rgba(255, 125, 40, 0.08)',
        'inset 0 1px 0 rgba(255, 180, 100, 0.06)',
        '0 4px 16px rgba(0, 0, 0, 0.35)',
        '0 6px 20px rgba(255, 120, 30, 0.12)',
      ].join(', ');

  const processingShadow = isWindow
    ? [
        '0 0 0 1px rgba(80, 140, 240, 0.06)',
        'inset 0 1px 0 rgba(140, 180, 255, 0.05)',
        '0 2px 6px rgba(0, 0, 0, 0.4)',
        '0 6px 24px rgba(60, 130, 240, 0.14)',
        '0 0 48px rgba(60, 130, 240, 0.07)',
      ].join(', ')
    : [
        '0 0 0 1px rgba(80, 140, 240, 0.06)',
        'inset 0 1px 0 rgba(140, 180, 255, 0.05)',
        '0 4px 16px rgba(0, 0, 0, 0.35)',
        '0 6px 20px rgba(60, 130, 240, 0.1)',
      ].join(', ');

  const pillAnimation = isRecording
    ? 'buttervoice-overlay-in 220ms ease-out, buttervoice-recording-glow 2.8s ease-in-out infinite'
    : 'buttervoice-overlay-in 220ms ease-out';

  return (
    <Box style={wrapperStyle}>
      <Box
        style={{
          animation: pillAnimation,
          width: OVERLAY_WIDTH,
          padding: '0.4rem 0.55rem',
          borderRadius: '999px',
          border: isRecording
            ? '1px solid rgba(255, 140, 50, 0.28)'
            : '1px solid rgba(100, 155, 240, 0.22)',
          background: 'rgba(10, 10, 12, 0.92)',
          backdropFilter: 'blur(16px) saturate(1.3)',
          WebkitBackdropFilter: 'blur(16px) saturate(1.3)',
          boxShadow: isRecording ? recordingShadow : processingShadow,
          transition: 'border-color 400ms ease, box-shadow 400ms ease',
        }}
      >
        <canvas
          ref={canvasRef}
          style={{
            width: '100%',
            height: '2.25rem',
            display: 'block',
          }}
        />
      </Box>
    </Box>
  );
}
