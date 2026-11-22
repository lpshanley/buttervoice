import { createSignal, onSettled } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";
import "./hud.css";

type Status = "idle" | "listening" | "working";

function Hud() {
  const [status, setStatus] = createSignal<Status>("idle");

  onSettled(() => {
    const listening = listen<Status>("dictation-status", (event) => setStatus(event.payload));
    return () => {
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
