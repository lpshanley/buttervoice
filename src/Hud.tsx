import { createSignal, onSettled } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import "./hud.css";

type Status = "idle" | "listening" | "working";

const LABELS: Record<Status, string> = {
  idle: "",
  listening: "Listening",
  working: "Working",
};

function Hud() {
  const [status, setStatus] = createSignal<Status>("idle");

  onSettled(() => {
    const listening = listen<Status>("dictation-status", (event) => setStatus(event.payload));
    return () => {
      void listening.then((stop) => stop());
    };
  });

  return (
    <div class={["hud", status()]} role="status" aria-live="polite">
      <span class="dot" />
      <span class="label">{LABELS[status()]}</span>
    </div>
  );
}

export default Hud;
