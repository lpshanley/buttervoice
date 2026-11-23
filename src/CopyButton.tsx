import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";

const SAY_COPIED_FOR_MS = 1500;

function CopyButton(props: { text: string }) {
  const [said, setSaid] = createSignal<"" | "copied" | "error.not_copied">("");
  async function copy() {
    try {
      await invoke("copy_text", { text: props.text });
      setSaid("copied");
    } catch {
      setSaid("error.not_copied");
    }
    setTimeout(() => setSaid(""), SAY_COPIED_FOR_MS);
  }
  return (
    <button type="button" class="quiet" onClick={() => void copy()}>
      {said() ? t(said() as "copied") : t("copy")}
    </button>
  );
}

export default CopyButton;
