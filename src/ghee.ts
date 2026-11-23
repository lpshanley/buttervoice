// How Ghee moves in the recording overlay: the "lean in" study from the
// brand's motion prototype. Each state has a pose Ghee moves toward;
// changes between states blend over a fifth of a second.
import { GHEE, type Command } from "./ghee-geometry";

export type Status = "idle" | "starting" | "listening" | "working" | "not_typed";

export type Pose = {
  angle: number;
  dy: number;
  fx: number;
  fy: number;
  eye: number;
  melt: number;
  mouth: number;
};

export const NEUTRAL: Pose = { angle: 0, dy: 0, fx: 0, fy: 0, eye: 1, melt: 0, mouth: 1 };

/** Added to every line of the brand drawing, in points: drawn as given it
 * looks faint beside the system's own icons and text. */
export const STROKE = 0.4;

/** Seconds a change of state takes to blend in. */
export const BLEND = 0.22;

const clamp = (v: number) => Math.max(0, Math.min(1, v));
export const smooth = (v: number) => {
  const c = clamp(v);
  return c * c * (3 - 2 * c);
};
const gauss = (v: number, width: number) => Math.exp(-0.5 * (v / width) ** 2);
const blink = (age: number) =>
  1 - 0.73 * Math.sin(Math.PI * clamp(((age % 5.7) - 2.91) / 0.16)) ** 2;

/** Where Ghee is heading `age` seconds into `status`. */
export function pose(status: Status, age: number): Pose {
  const p = { ...NEUTRAL };
  if (status === "starting") {
    // Eyes open as the microphone gets ready.
    const waking = 1 - smooth(age / 0.34);
    p.eye = 1 - 0.61 * waking;
    p.dy = 0.72 * waking;
  } else if (status === "listening") {
    // Leans in, settles, and holds still but for a blink.
    const look = smooth(age / 0.3);
    const settle = age > 0.3 ? Math.exp(-(age - 0.3) * 7) * Math.sin((age - 0.3) * 13) : 0;
    p.angle = -7 * look - 1.8 * settle;
    p.fx = -0.36 * look;
    p.fy = -0.44 * look;
    p.eye = blink(age);
  } else if (status === "working") {
    // The melts trade weight while the face stays level, from 300 ms on so
    // short jobs finish before it starts.
    const start = Math.max(0, age - 0.3);
    p.melt = smooth((age - 0.3) / 0.25) * Math.sin((2 * Math.PI * start) / 1.6);
  } else if (status === "not_typed") {
    // Glances up toward the menu bar and holds.
    const glance = smooth(age / 0.28);
    p.fx = 0.12 * glance;
    p.fy = -0.95 * glance;
    p.angle = 2.6 * glance;
    p.mouth = 1 - 0.45 * glance;
  }
  return p;
}

export function blend(from: Pose, to: Pose, t: number): Pose {
  const at = (key: keyof Pose) => from[key] + (to[key] - from[key]) * t;
  return {
    angle: at("angle"),
    dy: at("dy"),
    fx: at("fx"),
    fy: at("fy"),
    eye: at("eye"),
    melt: at("melt"),
    mouth: at("mouth"),
  };
}

const fixed = (n: number) => Number(n.toFixed(3));

/** Bends the lower edge: positive melt drops the left melt, lifts the right. */
function warp(x: number, y: number, melt: number) {
  return (
    y + melt * 1.03 * (gauss(x - 7.2, 2.9) - gauss(x - 16.5, 2.65)) * smooth((y - 13) / 4.3)
  );
}

function path(commands: Command[], melt = 0) {
  return commands
    .map(
      ([command, ...n]) =>
        command +
        n.map((v, i) => fixed(i % 2 ? warp(n[i - 1] ?? 0, v, melt) : v)).join(" "),
    )
    .join("");
}

export const PATHS = {
  outline: path(GHEE.outline),
  leftEye: path(GHEE["left-eye"]),
  rightEye: path(GHEE["right-eye"]),
  smile: path(GHEE.smile),
};

export function outline(melt: number) {
  return Math.abs(melt) < 0.0005 ? PATHS.outline : path(GHEE.outline, melt);
}

export const transforms = (p: Pose) => ({
  body: `translate(11 ${fixed(18.1 + p.dy)}) rotate(${fixed(p.angle)}) translate(-11 -18.1)`,
  face: `translate(${fixed(p.fx)} ${fixed(p.fy)})`,
  eyes: `translate(0 9.2) scale(1 ${fixed(p.eye)}) translate(0 -9.2)`,
  mouth: `translate(0 11.9) scale(1 ${fixed(p.mouth)}) translate(0 -11.9)`,
});
