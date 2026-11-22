import { createSignal, For, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  checkAccessibilityPermission,
  checkInputMonitoringPermission,
  checkMicrophonePermission,
  requestAccessibilityPermission,
  requestInputMonitoringPermission,
  requestMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { type Locale, t } from "./i18n";
import "./settings.css";

type Provider = "local" | "grok";

type State =
  | "starting"
  | "ready"
  | "model_missing"
  | "downloading"
  | "model_unusable"
  | "language_not_supported"
  | "ready_untested"
  | "key_from_environment"
  | "key_saved"
  | "no_key";

type Setup = { provider: Provider; state: State; model_bytes: number | null };

function megabytes(bytes: number) {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

const PROVIDERS: Provider[] = ["local", "grok"];

type LanguageSetup = { language: Locale; chosen: boolean };

// Each language is named in itself, so a person can find theirs whatever
// language the app happens to be showing.
const LANGUAGES: { tag: Locale; name: string }[] = [
  { tag: "en", name: "English" },
  { tag: "pt-BR", name: "Português (Brasil)" },
];

type Microphone = { id: string; name: string; built_in: boolean };

type MicrophoneSetup = { choice: string; connected: Microphone[] };

const DEVICE = "device:";

const STILL_STARTING_RECHECK_MS = 500;

type Permission = {
  id: "microphone" | "input_monitoring" | "accessibility";
  check: () => Promise<boolean>;
  request: () => Promise<unknown>;
};

const PERMISSIONS: Permission[] = [
  {
    id: "microphone",
    check: checkMicrophonePermission,
    request: requestMicrophonePermission,
  },
  {
    id: "input_monitoring",
    check: checkInputMonitoringPermission,
    request: requestInputMonitoringPermission,
  },
  {
    id: "accessibility",
    check: checkAccessibilityPermission,
    request: requestAccessibilityPermission,
  },
];

function Settings() {
  const [allowed, setAllowed] = createSignal<boolean[] | null>(null);
  let checkedBefore = false;

  async function recheck() {
    const now = await Promise.all(PERMISSIONS.map((permission) => permission.check()));
    const first = !checkedBefore;
    checkedBefore = true;
    setAllowed(now);
    // Something is missing at launch: bring the window up to say so.
    if (first && now.includes(false)) {
      const window = getCurrentWindow();
      await window.show();
      await window.setFocus();
    }
  }

  const [setup, setSetup] = createSignal<Setup>({
    provider: "local",
    state: "starting",
    model_bytes: null,
  });
  const [downloaded, setDownloaded] = createSignal(0);

  type Found = { current: string; newer: string | null };
  const [update, setUpdate] = createSignal<"idle" | "checking" | "installing">("idle");
  const [found, setFound] = createSignal<Found | null>(null);
  const [version, setVersion] = createSignal("");

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

  async function download() {
    setDownloaded(0);
    const settled = attempt("download_model");
    // Show the download as started without waiting for it to finish.
    await refresh();
    await settled;
  }
  const [draft, setDraft] = createSignal("");
  const [problem, setProblem] = createSignal("");

  async function refresh() {
    const now = await invoke<Setup>("transcription_setup");
    setSetup(now);
    // The model loads in the background at launch; look again until it has.
    if (now.state === "starting") setTimeout(() => void refresh(), STILL_STARTING_RECHECK_MS);
  }

  const [language, setLanguage] = createSignal<LanguageSetup>({ language: "en", chosen: true });
  let languageCheckedBefore = false;

  async function refreshLanguage() {
    const now = await invoke<LanguageSetup>("language_setup");
    const first = !languageCheckedBefore;
    languageCheckedBefore = true;
    setLanguage(now);
    // Language is the first thing to set up: bring the window up to ask.
    if (first && !now.chosen) {
      const window = getCurrentWindow();
      await window.show();
      await window.setFocus();
    }
  }

  async function chooseLanguage(tag: Locale) {
    await attempt("choose_language", { language: tag });
    await refreshLanguage();
  }

  const [microphones, setMicrophones] = createSignal<MicrophoneSetup>({
    choice: "built_in",
    connected: [],
  });

  async function refreshMicrophones() {
    setMicrophones(await invoke<MicrophoneSetup>("microphone_setup"));
  }

  async function chooseMicrophone(choice: string) {
    await attempt("choose_microphone", { choice });
    await refreshMicrophones();
  }

  // The built-in microphone is offered as its own choice, so it is not
  // listed a second time among the devices.
  const otherDevices = () => microphones().connected.filter((device) => !device.built_in);

  const chosenDeviceMissing = () => {
    const choice = microphones().choice;
    return (
      choice.startsWith(DEVICE) &&
      !microphones().connected.some((device) => DEVICE + device.id === choice)
    );
  };

  async function attempt(command: string, args?: Record<string, unknown>) {
    try {
      await invoke(command, args);
      setDraft("");
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    await refresh();
  }

  onSettled(() => {
    void refreshLanguage();
    void refresh();
    void refreshMicrophones();
    void getVersion().then(setVersion);
    void recheck();
    // macOS does not say when a permission changes. Coming back to this
    // window from System Settings is the moment to look again.
    const watching = getCurrentWindow().onFocusChanged((event) => {
      if (event.payload) {
        void recheck();
        // A headset may have been connected while the window was away.
        void refreshMicrophones();
      }
    });
    const progress = listen<number>("model-download", (event) => setDownloaded(event.payload));
    return () => {
      void watching.then((stop) => stop());
      void progress.then((stop) => stop());
    };
  });

  return (
    <main class="settings">
      <h1>ButterVoice</h1>
      <h2>{t("language.heading")}</h2>
      <Show when={!language().chosen}>
        <p class="status" role="status">
          {t("language.choose_first")}
        </p>
      </Show>
      <div class="providers" role="radiogroup" aria-label={t("language.heading")}>
        <For each={LANGUAGES}>
          {(option) => (
            <label>
              <input
                type="radio"
                name="language"
                checked={language().chosen && language().language === option.tag}
                onChange={() => void chooseLanguage(option.tag)}
              />
              <span>
                <strong>{option.name}</strong>
              </span>
            </label>
          )}
        </For>
      </div>
      <p class="status">{t("language.what")}</p>
      <h2>{t("transcription.heading")}</h2>
      <div class="providers" role="radiogroup" aria-label={t("transcription.choice_label")}>
        <For each={PROVIDERS}>
          {(provider) => (
            <label>
              <input
                type="radio"
                name="provider"
                checked={setup().provider === provider}
                onChange={() => void attempt("choose_provider", { provider })}
              />
              <span>
                <strong>{t(`provider.${provider}.name`)}</strong>
                <span class="why">{t(`provider.${provider}.what`)}</span>
              </span>
            </label>
          )}
        </For>
      </div>
      <p class="status" role="status">
        {t(`status.${setup().state}`)}
      </p>
      <Show when={setup().state === "model_missing" && setup().model_bytes}>
        <button type="button" onClick={() => void download()}>
          {t("download.button", { size: megabytes(setup().model_bytes ?? 0) })}
        </button>
        <p class="status">{t("download.what")}</p>
      </Show>
      <Show when={setup().state === "downloading"}>
        <progress max={setup().model_bytes ?? 1} value={downloaded()} />
        <p class="status">
          {t("download.progress", {
            done: megabytes(downloaded()),
            size: megabytes(setup().model_bytes ?? 0),
          })}
        </p>
        <button
          type="button"
          class="quiet"
          onClick={() => void invoke("cancel_model_download")}
        >
          {t("download.cancel")}
        </button>
      </Show>
      <Show when={setup().provider === "grok"}>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void attempt("save_api_key", { key: draft() });
          }}
        >
          <label for="api-key">{t("key.label")}</label>
          <div class="row">
            <input
              id="api-key"
              type="password"
              autocomplete="off"
              spellcheck="false"
              placeholder={t("key.placeholder")}
              value={draft()}
              onInput={(event) => setDraft(event.currentTarget.value)}
            />
            <button type="submit">{t("key.save")}</button>
          </div>
        </form>
      </Show>
      <Show when={setup().state === "key_saved"}>
        <button type="button" class="quiet" onClick={() => void attempt("forget_api_key")}>
          {t("key.forget")}
        </button>
      </Show>
      <Show when={problem()}>
        <p class="problem" role="alert">
          {problem()}
        </p>
      </Show>
      <h2>{t("microphone.heading")}</h2>
      <div class="providers" role="radiogroup" aria-label={t("microphone.heading")}>
        <label>
          <input
            type="radio"
            name="microphone"
            checked={microphones().choice === "built_in"}
            onChange={() => void chooseMicrophone("built_in")}
          />
          <span>
            <strong>{t("microphone.built_in")}</strong>
            <span class="why">{t("microphone.built_in_what")}</span>
          </span>
        </label>
        <label>
          <input
            type="radio"
            name="microphone"
            checked={microphones().choice === "system_default"}
            onChange={() => void chooseMicrophone("system_default")}
          />
          <span>
            <strong>{t("microphone.system_default")}</strong>
            <span class="why">{t("microphone.system_default_what")}</span>
          </span>
        </label>
        <For each={otherDevices()}>
          {(device) => (
            <label>
              <input
                type="radio"
                name="microphone"
                checked={microphones().choice === DEVICE + device.id}
                onChange={() => void chooseMicrophone(DEVICE + device.id)}
              />
              <span>
                <strong>{device.name}</strong>
              </span>
            </label>
          )}
        </For>
        <Show when={chosenDeviceMissing()}>
          <label>
            <input type="radio" name="microphone" checked disabled />
            <span>
              <strong>{t("microphone.chosen_device")}</strong>
              <span class="why">{t("microphone.not_connected")}</span>
            </span>
          </label>
        </Show>
      </div>
      <h2>{t("update.heading")}</h2>
      <p class="status">{t("update.current", { version: version() })}</p>
      <Show
        when={found()?.newer}
        fallback={
          <>
            <button
              type="button"
              disabled={update() !== "idle"}
              onClick={() => void checkForUpdate()}
            >
              {update() === "checking" ? t("update.checking") : t("update.check")}
            </button>
            <Show when={found() && !found()?.newer}>
              <p class="status" role="status">
                {t("update.newest")}
              </p>
            </Show>
          </>
        }
      >
        <p class="status" role="status">
          {t("update.available", { version: found()?.newer ?? "" })}
        </p>
        <button
          type="button"
          disabled={update() !== "idle"}
          onClick={() => void installUpdate()}
        >
          {update() === "installing" ? t("update.installing") : t("update.install")}
        </button>
      </Show>
      <p class="status">{t("update.what")}</p>
      <h2>{t("permissions.heading")}</h2>
      <ul class="permissions">
        <For each={PERMISSIONS}>
          {(permission, index) => (
            <li>
              <div>
                <strong>{t(`permission.${permission.id}.name`)}</strong>
                <span class="why">{t(`permission.${permission.id}.why`)}</span>
              </div>
              <Show
                when={allowed()?.[index()] === false}
                fallback={<span class="allowed">{allowed() ? t("permissions.allowed") : ""}</span>}
              >
                <button
                  type="button"
                  onClick={() => void permission.request().then(() => recheck())}
                >
                  {t("permissions.allow")}
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
    </main>
  );
}

export default Settings;
