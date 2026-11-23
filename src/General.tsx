import { createSignal, For, onSettled, Show, useContext } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import GheeMascot from "./GheeMascot";
import { t } from "./i18n";
import { PERMISSIONS, ReadinessContext } from "./readiness";

type Found = { current: string; newer: string | null };

/** The app itself: its updates and the access macOS gives it. */
function General() {
  const readiness = useContext(ReadinessContext);
  const allowed = readiness.allowed;
  const missing = () => PERMISSIONS.filter((_, index) => allowed()?.[index] === false);

  const [update, setUpdate] = createSignal<"idle" | "checking" | "installing">("idle");
  const [found, setFound] = createSignal<Found | null>(null);
  const [version, setVersion] = createSignal("");
  const [daily, setDaily] = createSignal(true);
  const [problem, setProblem] = createSignal("");

  async function refreshUpdates() {
    const now = await invoke<{ daily: boolean; known: string | null }>("update_setup");
    setDaily(now.daily);
    // A daily check may already have found one while this window was shut.
    if (now.known) setFound({ current: version(), newer: now.known });
  }

  async function checkForUpdate() {
    setUpdate("checking");
    try {
      setFound(await invoke<Found>("check_for_update"));
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    setUpdate("idle");
  }

  async function installUpdate() {
    setUpdate("installing");
    try {
      // On success the app relaunches and this window goes with it.
      await invoke("install_update");
    } catch (error) {
      setProblem(String(error));
      setUpdate("idle");
    }
  }

  async function chooseDaily(on: boolean) {
    try {
      await invoke("choose_daily_update_check", { on });
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    await refreshUpdates();
  }

  onSettled(() => {
    void getVersion()
      .then(setVersion)
      .then(() => refreshUpdates());
    const updateKnown = listen("update-known", () => void refreshUpdates());
    return () => void updateKnown.then((stop) => stop());
  });

  return (
    <div class="settings">
      <h2>{t("update.heading")}</h2>
      <div class="update-status">
        <GheeMascot size={36} />
        <div class="what">
          <span>
            {found()?.newer
              ? t("update.ready_title", { version: found()?.newer ?? "" })
              : found()
                ? t("update.newest")
                : "ButterVoice"}
          </span>
          <span class="why">{t("update.version", { version: version() })}</span>
        </div>
        <Show
          when={found()?.newer}
          fallback={
            <button
              type="button"
              class="soft"
              disabled={update() !== "idle"}
              onClick={() => void checkForUpdate()}
            >
              {update() === "checking" ? t("update.checking") : t("update.check")}
            </button>
          }
        >
          <button
            type="button"
            class="pill"
            disabled={update() !== "idle"}
            onClick={() => void installUpdate()}
          >
            {update() === "installing" ? t("update.installing_short") : t("update.button")}
          </button>
        </Show>
      </div>
      <Show when={problem()}>
        <p class="problem" role="alert">
          {problem()}
        </p>
      </Show>
      <ul class="permissions">
        <li>
          <label for="daily-updates">{t("update.daily")}</label>
          <input
            id="daily-updates"
            type="checkbox"
            checked={daily()}
            onChange={(event) => void chooseDaily(event.currentTarget.checked)}
          />
        </li>
      </ul>
      <h2 id="permissions">{t("permissions.heading")}</h2>
      <Show
        when={missing().length > 0}
        fallback={<p class="status">{allowed() ? t("permissions.all_allowed") : ""}</p>}
      >
        <ul class="permissions">
          <For each={missing()}>
            {(permission) => (
              <li>
                <div>
                  <strong>{t(`permission.${permission.id}.name`)}</strong>
                  <span class="why">{t(`permission.${permission.id}.why`)}</span>
                </div>
                <button
                  type="button"
                  onClick={() =>
                    void permission.request().then(() => readiness.recheckPermissions())
                  }
                >
                  {t("permissions.allow")}
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
}

export default General;
