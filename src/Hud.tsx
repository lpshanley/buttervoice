import { createSignal, onSettled } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";
import "./hud.css";

type Status = "idle" | "starting" | "listening" | "working" | "not_typed";

/** How long "not typed" stays before the HUD fades, unless a dictation starts. */
const NOT_TYPED_FOR_MS = 4000;

function Hud() {
  const [status, setStatus] = createSignal<Status>("idle");

  onSettled(() => {
    let fading: ReturnType<typeof setTimeout> | undefined;
    const listening = listen<Status>("dictation-status", (event) => {
      clearTimeout(fading);
      setStatus(event.payload);
      if (event.payload === "not_typed") {
        fading = setTimeout(() => setStatus("idle"), NOT_TYPED_FOR_MS);
      }
    });
    return () => {
      clearTimeout(fading);
      void listening.then((stop) => stop());
    };
  });

  const label = () => {
    const now = status();
    return now === "idle" ? "" : t(`hud.${now}`);
  };

  return (
    <div class={["hud", status()]} role="status" aria-live="polite">
      <span class="dot" />
      <span class="label">{label()}</span>
    </div>
  );
}

export default Hud;
