import { createSignal, onSettled, Show } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { BLEND, NEUTRAL, PATHS, STROKE, blend, outline, pose, smooth, transforms } from "./ghee";
import type { Pose, Status } from "./ghee";
import { t } from "./i18n";
import "./hud.css";

/** How long "couldn't type here" stays before the HUD fades, unless a dictation starts. */
const NOT_TYPED_FOR_MS = 4000;

function Hud() {
  const [status, setStatus] = createSignal<Status>("idle");
  let body: SVGGElement | undefined;
  let edge: SVGPathElement | undefined;
  let face: SVGGElement | undefined;
  let eyes: SVGGElement | undefined;
  let mouth: SVGPathElement | undefined;

  onSettled(() => {
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
    // Where Ghee was when the state last changed, and when that was.
    let entered = { status: "idle" as Status, at: performance.now(), from: NEUTRAL };
    let shown = NEUTRAL;
    let frame = 0;
    let fading: ReturnType<typeof setTimeout> | undefined;

    const draw = (p: Pose) => {
      const place = transforms(p);
      body?.setAttribute("transform", place.body);
      edge?.setAttribute("d", outline(p.melt));
      face?.setAttribute("transform", place.face);
      eyes?.setAttribute("transform", place.eyes);
      mouth?.setAttribute("transform", place.mouth);
      shown = p;
    };

    const animate = (now: number) => {
      const age = (now - entered.at) / 1000;
      draw(blend(entered.from, pose(entered.status, age), smooth(age / BLEND)));
      // Once faded out and settled there is nothing left to move.
      frame = entered.status === "idle" && age > BLEND ? 0 : requestAnimationFrame(animate);
    };

    const enter = (next: Status) => {
      entered = { status: next, at: performance.now(), from: shown };
      if (reduced.matches) {
        cancelAnimationFrame(frame);
        frame = 0;
        draw(NEUTRAL);
      } else if (!frame) {
        frame = requestAnimationFrame(animate);
      }
    };

    const listening = listen<Status>("dictation-status", (event) => {
      clearTimeout(fading);
      setStatus(event.payload);
      enter(event.payload);
      if (event.payload === "not_typed") {
        fading = setTimeout(() => {
          setStatus("idle");
          enter("idle");
        }, NOT_TYPED_FOR_MS);
      }
    });
    return () => {
      clearTimeout(fading);
      cancelAnimationFrame(frame);
      void listening.then((stop) => stop());
    };
  });

  const label = () => {
    const now = status();
    return now === "idle" ? "" : t(`hud.${now}`);
  };

  return (
    <div class={["hud", status()]} role="status" aria-live="polite">
      <svg
        width="22"
        height="22"
        viewBox="0 0 22 22"
        fill="currentColor"
        stroke="currentColor"
        stroke-width={STROKE}
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <g ref={(el) => (body = el)}>
          <path ref={(el) => (edge = el)} fill-rule="evenodd" d={PATHS.outline} />
          <g ref={(el) => (face = el)}>
            <g ref={(el) => (eyes = el)}>
              <path d={PATHS.leftEye} />
              <path d={PATHS.rightEye} />
            </g>
            <path ref={(el) => (mouth = el)} d={PATHS.smile} />
          </g>
        </g>
      </svg>
      <span class="label">
        {label()}
        <Show when={status() === "not_typed"}>
          <span class="action">{t("hud.not_typed_action")}</span>
        </Show>
      </span>
    </div>
  );
}

export default Hud;
