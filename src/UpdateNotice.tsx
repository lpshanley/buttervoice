import { createSignal, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";

/** At the foot of the sidebar, only while a newer version is waiting. */
function UpdateNotice() {
  const [newer, setNewer] = createSignal<string | null>(null);
  const [installing, setInstalling] = createSignal(false);
  // How much has downloaded; `null` until the first report, or when the size is unknown.
  const [percent, setPercent] = createSignal<number | null>(null);
  const [problem, setProblem] = createSignal("");

  async function install() {
    setProblem("");
    setInstalling(true);
    try {
      // On success the app relaunches into the new version.
      await invoke("install_update");
    } catch (error) {
      setProblem(String(error));
      setInstalling(false);
      setPercent(null);
    }
  }

  onSettled(() => {
    void invoke<{ known: string | null }>("update_setup").then((now) => setNewer(now.known));
    const known = listen<string | null>("update-known", (event) => setNewer(event.payload));
    // Also arrives when Settings started the update.
    const progress = listen<number | null>("update-progress", (event) => setPercent(event.payload));
    return () => {
      void known.then((stop) => stop());
      void progress.then((stop) => stop());
    };
  });

  return (
    <Show when={newer()}>
      {(version) => (
        <Show
          when={installing() || percent() !== null}
          fallback={
            <div class="update" role="status" title={problem() || undefined}>
              <span>{t("update.ready_title", { version: version() })}</span>
              <button
                type="button"
                class={["pill", { failed: Boolean(problem()) }]}
                onClick={() => void install()}
              >
                {problem() ? t("update.retry") : t("update.button")}
              </button>
            </div>
          }
        >
          <div class="update updating" role="status">
            <div class="label">
              <span>
                {percent() === 100
                  ? t("update.restarting")
                  : t("update.updating_to", { version: version() })}
              </span>
              <Show when={percent() !== null && percent() !== 100}>
                <span class="percent">{percent()}%</span>
              </Show>
            </div>
            <div
              class="bar"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={percent() ?? undefined}
            >
              <div style={{ width: `${percent() ?? 0}%` }} />
            </div>
          </div>
        </Show>
      )}
    </Show>
  );
}

export default UpdateNotice;
