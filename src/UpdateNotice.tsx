import { createSignal, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";

/** At the foot of the sidebar, only while a newer version is waiting. */
function UpdateNotice() {
  const [newer, setNewer] = createSignal<string | null>(null);
  const [installing, setInstalling] = createSignal(false);
  const [problem, setProblem] = createSignal("");

  async function install() {
    setInstalling(true);
    try {
      // On success the app relaunches into the new version.
      await invoke("install_update");
    } catch (error) {
      setProblem(String(error));
      setInstalling(false);
    }
  }

  onSettled(() => {
    void invoke<{ known: string | null }>("update_setup").then((now) => setNewer(now.known));
    const known = listen<string | null>("update-known", (event) => setNewer(event.payload));
    return () => void known.then((stop) => stop());
  });

  return (
    <Show when={newer()}>
      <div class="update-notice" role="status">
        <p>{t("update.ready", { version: newer() ?? "" })}</p>
        <button type="button" disabled={installing()} onClick={() => void install()}>
          {installing() ? t("update.installing_short") : t("update.install_short")}
        </button>
        <Show when={problem()}>
          <p class="problem">{problem()}</p>
        </Show>
      </div>
    </Show>
  );
}

export default UpdateNotice;
