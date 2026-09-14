import { useEffect, useRef } from 'react';
import { useAtomValue } from 'jotai';
import { Box } from '@mantine/core';
import { dictationStateAtom, inputLevelAtom } from '../../stores/app';

type OverlayPresentation = 'shell' | 'window';
type Rgb = readonly [number, number, number];

const OVERLAY_WIDTH = 'min(12rem, calc(100vw - 0.75rem))';
const PROCESSING_STATES = ['transcribing', 'post_processing', 'injecting'] as const;

/** Duration of the recording -> processing canvas crossfade. */
const BLEND_MS = 350;

/* ─────────────────────────────────────────────────────────────────────────────
 * Palettes
 *
 * Recording follows the app's `butter` theme ramp (see src/main.tsx) so the
 * HUD reads as part of the brand. Processing stays cool for at-a-glance
 * contrast, but in a soft periwinkle that sits comfortably next to gold.
 * ───────────────────────────────────────────────────────────────────────── */

const RECORDING_PALETTE = {
  /** butter.2 — rear, widest layer */
  back: [253, 226, 138] as Rgb,
  /** butter.3 — middle layer */
  mid: [251, 208, 77] as Rgb,
  /** butter.5 — front, most saturated layer */
  front: [232, 163, 8] as Rgb,
  /** butter.1 — highlight stroke along the front edge */
  highlight: [254, 240, 199] as Rgb,
  /** butter.6 — deeper amber toward the pill ends */
  edge: [200, 125, 4] as Rgb,
  /** butter.3 — glow / chrome tint */
  glow: [251, 208, 77] as Rgb,
} as const;

const PROCESSING_PALETTE = {
  /** resting bar colour */
  base: [96, 108, 190] as Rgb,
  /** colour as the luminous sweep passes */
  lit: [176, 186, 255] as Rgb,
  /** glow / chrome tint */
  glow: [130, 150, 250] as Rgb,
} as const;

function rgba([r, g, b]: Rgb, a: number): string {
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

function mix(a: Rgb, b: Rgb, t: number): Rgb {
  return [
    Math.round(a[0] + (b[0] - a[0]) * t),
    Math.round(a[1] + (b[1] - a[1]) * t),
    Math.round(a[2] + (b[2] - a[2]) * t),
  ];
}

/* ─────────────────────────────────────────────────────────────────────────────
 * Recording Visualization — "Ember Flow"
 *
 * Three overlapping waveform layers in butter/gold tones. Each layer has a
 * distinct frequency, phase speed, and opacity, creating organic depth. A
 * soft radial core glow sits behind the layers and swells with input level,
 * the front layer gets a pale highlight stroke, and the whole wave dissolves
 * into the pill ends via an alpha mask. Callers clear the canvas.
 * ───────────────────────────────────────────────────────────────────────── */

interface RecordingFrame {
  phase: number;
  /** Smoothed, perceptually-shaped input level in [0, 1]. */
  energy: number;
  /** 0–1 draw opacity (used for the state crossfade). */
  opacity: number;
}

function drawRecording(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  { phase, energy, opacity }: RecordingFrame,
) {
  if (opacity <= 0.005) return;

  const midY = h / 2;
  const amp = energy * midY * 0.82;
  const { back, mid, front, highlight, edge } = RECORDING_PALETTE;

  ctx.save();

  // Soft radial core glow behind the wave — swells with level
  const coreR = w * (0.22 + energy * 0.2);
  const core = ctx.createRadialGradient(w / 2, midY, 0, w / 2, midY, coreR);
  core.addColorStop(0, rgba(mid, (0.05 + energy * 0.2) * opacity));
  core.addColorStop(0.6, rgba(front, (0.02 + energy * 0.08) * opacity));
  core.addColorStop(1, rgba(front, 0));
  ctx.fillStyle = core;
  ctx.fillRect(0, 0, w, h);

  const layers = [
    { freq: [2.2, 4.8], spd: [0.6, 0.85], scale: 0.45, rgb: back, a: 0.22 },
    { freq: [3.2, 5.5], spd: [1.0, 1.3], scale: 0.7, rgb: mid, a: 0.38 },
    { freq: [3.8, 7.2], spd: [1.25, 0.7], scale: 1.0, rgb: front, a: 0.58 },
  ];

  let lastTopY: number[] = [];

  for (const layer of layers) {
    const a = amp * layer.scale;

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
    ctx.shadowColor = rgba(layer.rgb, (0.1 + energy * 0.22) * opacity);
    ctx.shadowBlur = 3 + energy * 12;

    // Symmetric filled waveform
    ctx.beginPath();
    ctx.moveTo(0, midY);
    for (let x = 0; x <= w; x++) ctx.lineTo(x, topY[x]);
    ctx.lineTo(w, midY);
    for (let x = w; x >= 0; x--) ctx.lineTo(x, midY + (midY - topY[x]));
    ctx.closePath();

    // Horizontal gradient: bright at the centre, deeper amber toward the ends
    const la = layer.a * opacity;
    const grad = ctx.createLinearGradient(0, 0, w, 0);
    grad.addColorStop(0, rgba(mix(layer.rgb, edge, 0.7), la * 0.55));
    grad.addColorStop(0.5, rgba(layer.rgb, la));
    grad.addColorStop(1, rgba(mix(layer.rgb, edge, 0.7), la * 0.55));
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.restore();

    lastTopY = topY;
  }

  // Highlight stroke along the front layer's upper edge
  if (lastTopY.length > 0) {
    ctx.beginPath();
    ctx.moveTo(0, midY);
    for (let x = 0; x <= w; x++) ctx.lineTo(x, lastTopY[x]);
    ctx.strokeStyle = rgba(highlight, (0.14 + energy * 0.34) * opacity);
    ctx.lineWidth = 0.9;
    ctx.stroke();
  }

  // Edge fade: dissolve the wave before it reaches the rounded pill ends
  ctx.globalCompositeOperation = 'destination-in';
  const fade = ctx.createLinearGradient(0, 0, w, 0);
  fade.addColorStop(0, 'rgba(0, 0, 0, 0)');
  fade.addColorStop(0.14, 'rgba(0, 0, 0, 1)');
  fade.addColorStop(0.86, 'rgba(0, 0, 0, 1)');
  fade.addColorStop(1, 'rgba(0, 0, 0, 0)');
  ctx.fillStyle = fade;
  ctx.fillRect(0, 0, w, h);

  ctx.restore();
}

/* ─────────────────────────────────────────────────────────────────────────────
 * Processing Visualization — "Cascade Bars"
 *
 * A row of rounded bars in periwinkle tones, each with a lighter cap. Two
 * luminous sweeps travel across at different speeds, causing bars to surge
 * in height and brightness as they pass. Wrap-aware distance keeps the loop
 * seamless; a gentle breathing oscillation keeps bars alive between sweeps.
 * Callers clear the canvas.
 * ───────────────────────────────────────────────────────────────────────── */

interface ProcessingFrame {
  time: number;
  opacity: number;
  reducedMotion: boolean;
}

function drawProcessing(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  { time, opacity, reducedMotion }: ProcessingFrame,
) {
  if (opacity <= 0.005) return;

  const midY = h / 2;
  const count = 22;
  const gap = 2.5;
  const barW = (w - gap * (count - 1)) / count;
  const maxH = h * 0.56;
  const { base, lit, glow } = PROCESSING_PALETTE;

  // Two sweeps at different speeds, wrapping smoothly
  const s1 = (time * 0.00042) % 1;
  const s2 = (time * 0.00028 + 0.5) % 1;

  for (let i = 0; i < count; i++) {
    const t = i / (count - 1);
    const x = i * (barW + gap);

    let sweep = 0;
    if (!reducedMotion) {
      const rawD1 = Math.abs(t - s1);
      const rawD2 = Math.abs(t - s2);
      const d1 = Math.min(rawD1, 1 - rawD1);
      const d2 = Math.min(rawD2, 1 - rawD2);
      const sw1 = Math.max(0, 1 - d1 * 5);
      const sw2 = Math.max(0, 1 - d2 * 6);
      sweep = Math.max(sw1 * sw1, sw2 * sw2 * 0.55);
    }

    // Gentle breathing + sweep-driven surge
    const breath = 0.14 + Math.sin(t * Math.PI * 2.5 + time * 0.002) * 0.05;
    const barH = (breath + sweep * 0.7) * maxH;

    const body = mix(base, lit, sweep);
    const cap = mix(body, lit, 0.5);
    const alpha = (0.3 + sweep * 0.62) * opacity;

    const y = midY - barH / 2;
    const radius = Math.min(barW * 0.32, 2.5);

    ctx.save();
    if (sweep > 0.12) {
      ctx.shadowColor = rgba(glow, sweep * 0.38 * opacity);
      ctx.shadowBlur = 4 + sweep * 12;
    }
    const grad = ctx.createLinearGradient(0, y, 0, y + barH);
    grad.addColorStop(0, rgba(cap, alpha));
    grad.addColorStop(0.45, rgba(body, alpha));
    grad.addColorStop(1, rgba(body, alpha * 0.8));
    ctx.fillStyle = grad;
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

  const frameRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animRef = useRef<number>(0);
  const phaseRef = useRef(0);
  const smoothLevelRef = useRef(0);
  const levelRef = useRef(0);
  const startTimeRef = useRef(0);
  const recordingRef = useRef(isRecording);
  /** 0 = fully recording visual, 1 = fully processing visual. */
  const blendRef = useRef(isRecording ? 0 : 1);

  levelRef.current = Math.max(0, Math.min(100, inputLevel)) / 100;
  recordingRef.current = isRecording;

  // HUD window visibility is owned by the Rust side (emit_state in
  // app_state.rs); showing/hiding from the webview raced with it and could
  // leave the window orphaned on screen.

  // Canvas animation loop — a single loop for both states so the
  // recording -> processing switch can crossfade instead of cutting.
  useEffect(() => {
    if (!visible) {
      smoothLevelRef.current = 0;
      return;
    }
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const reducedMotion =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const w = rect.width;
    const h = rect.height;
    startTimeRef.current = performance.now();
    // Enter directly in the current state — no crossfade on appearance.
    blendRef.current = recordingRef.current ? 0 : 1;
    let lastFrame = startTimeRef.current;

    const draw = (now: number) => {
      const dt = Math.min(64, now - lastFrame);
      lastFrame = now;
      const elapsed = now - startTimeRef.current;

      // Crossfade between the two visuals
      const target = recordingRef.current ? 0 : 1;
      const step = dt / BLEND_MS;
      if (blendRef.current < target) blendRef.current = Math.min(target, blendRef.current + step);
      else if (blendRef.current > target) blendRef.current = Math.max(target, blendRef.current - step);
      const blend = blendRef.current;

      // Perceptual level shaping + asymmetric smoothing: blooms quickly on
      // syllables (attack) and settles gracefully afterwards (release).
      const shaped = recordingRef.current ? Math.pow(levelRef.current, 0.6) : 0;
      const coeff = shaped > smoothLevelRef.current ? 0.35 : 0.08;
      smoothLevelRef.current += (shaped - smoothLevelRef.current) * coeff;
      const smooth = smoothLevelRef.current;

      // Idle shimmer: a slow, low-amplitude breath so silence isn't a flat line
      const quiet = 1 - Math.min(1, smooth * 4);
      const shimmer = reducedMotion
        ? 0.03
        : (0.5 + 0.5 * Math.sin(elapsed * 0.0016)) * 0.06 * quiet;
      const energy = Math.min(1, Math.max(0.06, smooth + shimmer));

      // Voice-reactive chrome glow (consumed by the glow layer's opacity)
      frameRef.current?.style.setProperty('--bv-level', energy.toFixed(3));

      ctx.clearRect(0, 0, w, h);
      drawRecording(ctx, w, h, { phase: phaseRef.current, energy, opacity: 1 - blend });
      drawProcessing(ctx, w, h, { time: elapsed, opacity: blend, reducedMotion });

      // Level-driven phase speed: the wave travels faster as you speak
      const speed = (0.045 + smooth * 0.05) * (reducedMotion ? 0.5 : 1);
      phaseRef.current -= speed * (dt / 16.67);

      animRef.current = requestAnimationFrame(draw);
    };

    animRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animRef.current);
  }, [visible]);

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

  const tintRgb = isRecording ? RECORDING_PALETTE.glow : PROCESSING_PALETTE.glow;

  // Static base shadow: depth + a whisper of tint. The voice-reactive part
  // lives on a separate glow layer so it can animate via opacity alone.
  const baseShadow = [
    `inset 0 1px 0 ${rgba(tintRgb, 0.07)}`,
    isWindow ? '0 2px 6px rgba(0, 0, 0, 0.42)' : '0 4px 16px rgba(0, 0, 0, 0.35)',
    `0 6px 20px ${rgba(tintRgb, 0.08)}`,
  ].join(', ');

  const glowShadow = isWindow
    ? [`0 6px 26px ${rgba(tintRgb, 0.3)}`, `0 0 52px ${rgba(tintRgb, 0.18)}`].join(', ')
    : [`0 6px 22px ${rgba(tintRgb, 0.26)}`, `0 0 40px ${rgba(tintRgb, 0.14)}`].join(', ');

  // Gradient border ring: transparent border + two backgrounds, one clipped
  // to the padding box (warm glass) and one to the border box (gold ring).
  const ring = `linear-gradient(180deg, ${rgba(tintRgb, 0.36)}, ${rgba(tintRgb, 0.07)})`;
  const glass = 'linear-gradient(180deg, rgba(24, 19, 12, 0.94), rgba(10, 9, 8, 0.97))';

  return (
    <Box style={wrapperStyle}>
      <Box
        ref={frameRef}
        style={{
          position: 'relative',
          width: OVERLAY_WIDTH,
          animation: 'buttervoice-overlay-in 320ms cubic-bezier(0.22, 1, 0.36, 1) both',
        }}
      >
        <Box
          aria-hidden
          style={{
            position: 'absolute',
            inset: 0,
            borderRadius: '999px',
            boxShadow: glowShadow,
            opacity: isRecording ? 'calc(0.2 + var(--bv-level, 0) * 0.8)' : 0.55,
            transition: 'box-shadow 400ms ease, opacity 400ms ease',
          }}
        />
        <Box
          style={{
            position: 'relative',
            padding: '0.4rem 0.55rem',
            borderRadius: '999px',
            border: '1px solid transparent',
            background: `${glass} padding-box, ${ring} border-box`,
            backdropFilter: 'blur(16px) saturate(1.3)',
            WebkitBackdropFilter: 'blur(16px) saturate(1.3)',
            boxShadow: baseShadow,
            transition: 'box-shadow 400ms ease',
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
    </Box>
  );
}
