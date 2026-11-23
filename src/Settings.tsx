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
import { count, minutes } from "./format";
import { type Locale, t } from "./i18n";

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

/** A provider is ready to dictate in these states. */
const READY: State[] = ["ready", "ready_untested", "key_from_environment", "key_saved"];

const PROVIDERS: Provider[] = ["local", "grok"];

type LanguageSetup = { language: Locale; chosen: boolean };

// Each language is named in itself, so a person can find theirs whatever
// language the app happens to be showing.
const LANGUAGES: { tag: Locale; name: string }[] = [
  { tag: "en", name: "English" },
  { tag: "pt-BR", name: "Português (Brasil)" },
];

type LocalModel = {
  id: string;
  name: string;
  bytes: number;
  vetted: boolean;
  recommended: boolean;
  in_use: boolean;
  downloaded: boolean;
};

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

/** Sent on the window when something must be set up before dictating. */
export const SETUP_NEEDED = "setup-needed";

function bringUpTheWindow() {
  window.dispatchEvent(new Event(SETUP_NEEDED));
  const current = getCurrentWindow();
  return current.show().then(() => current.setFocus());
}

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
      await bringUpTheWindow();
    }
  }

  const [setup, setSetup] = createSignal<Setup>({
    provider: "local",
    state: "starting",
    model_bytes: null,
  });
  const [downloaded, setDownloaded] = createSignal(0);
  const [models, setModels] = createSignal<LocalModel[]>([]);

  async function refreshModels() {
    setModels(await invoke<LocalModel[]>("local_models"));
  }

  async function chooseModel(model: LocalModel) {
    // Choosing the recommended model goes back to following the
    // recommendation, so a better one later is picked up by itself.
    await attempt("choose_local_model", { id: model.recommended ? null : model.id });
    await refreshModels();
  }

  type Found = { current: string; newer: string | null };
  const [update, setUpdate] = createSignal<"idle" | "checking" | "installing">("idle");
  const [found, setFound] = createSignal<Found | null>(null);
  const [version, setVersion] = createSignal("");
  const [daily, setDaily] = createSignal(true);

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
    void refreshModels();
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
      await bringUpTheWindow();
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

  // What is left before the first dictation, worked out from what this
  // window already knows. It goes once all three are done.
  const steps = () => [
    { id: "language", done: language().chosen },
    { id: "transcription", done: READY.includes(setup().state) },
    { id: "permissions", done: allowed()?.every(Boolean) ?? false },
  ];
  const settingUp = () => allowed() !== null && steps().some((step) => !step.done);

  type TimeSaved = {
    days: number;
    words_per_minute: number;
    words: number;
    spoken_ms: number;
    saved_ms: number;
  };
  const [saved, setSaved] = createSignal<TimeSaved | null>(null);

  async function refreshTimeSaved() {
    setSaved(await invoke<TimeSaved>("time_saved"));
  }

  async function chooseTypingSpeed(text: string) {
    const wordsPerMinute = Math.round(Number(text));
    if (wordsPerMinute > 0) await attempt("choose_typing_speed", { wordsPerMinute });
    await refreshTimeSaved();
  }

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
    void getVersion()
      .then(setVersion)
      .then(() => refreshUpdates());
    void recheck();
    void refreshTimeSaved();
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
    const finished = listen("dictation-finished", () => void refreshTimeSaved());
    return () => {
      void watching.then((stop) => stop());
      void progress.then((stop) => stop());
      void finished.then((stop) => stop());
    };
  });

  return (
    <div class="settings">
      <h2>{t("nav.settings")}</h2>
      <Show when={settingUp()}>
        <h2>{t("setup.heading")}</h2>
        <ol class="steps">
          <For each={steps()}>
            {(step) => (
              <li class={{ done: step.done }}>
                <span>{t(`setup.${step.id as "language" | "transcription" | "permissions"}`)}</span>
                <Show
                  when={!step.done}
                  fallback={<span class="allowed">{t("setup.done")}</span>}
                >
                  <button
                    type="button"
                    class="quiet"
                    onClick={() => document.getElementById(step.id)?.scrollIntoView({ behavior: "smooth" })}
                  >
                    {t("setup.show")}
                  </button>
                </Show>
              </li>
            )}
          </For>
        </ol>
        <p class="status">{t("setup.then")}</p>
      </Show>
      <h2 id="language">{t("language.heading")}</h2>
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
                <strong lang={option.tag}>{option.name}</strong>
              </span>
            </label>
          )}
        </For>
      </div>
      <p class="status">{t("language.what")}</p>
      <h2 id="transcription">{t("transcription.heading")}</h2>
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
      <Show when={setup().provider === "local" && models().length > 1}>
        <h2>{t("model.heading")}</h2>
        <div class="providers" role="radiogroup" aria-label={t("model.heading")}>
          <For each={models()}>
            {(model) => (
              <label>
                <input
                  type="radio"
                  name="local-model"
                  checked={model.in_use}
                  onChange={() => void chooseModel(model)}
                />
                <span>
                  <strong>{model.name}</strong>
                  <span class="why">
                    {[
                      model.recommended ? t("model.recommended") : "",
                      model.vetted ? "" : t("model.untested"),
                      model.downloaded ? "" : t("model.not_downloaded"),
                      megabytes(model.bytes),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </span>
              </label>
            )}
          </For>
        </div>
      </Show>
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
      <label class="switch">
        <input
          type="checkbox"
          checked={daily()}
          onChange={(event) =>
            void attempt("choose_daily_update_check", { on: event.currentTarget.checked }).then(
              () => refreshUpdates(),
            )
          }
        />
        <span>{t("update.daily")}</span>
      </label>
      <p class="status">{t("update.what")}</p>
      <h2 id="permissions">{t("permissions.heading")}</h2>
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
      <Show when={saved()}>
        {(now) => (
          <>
            <h2>{t("saved.heading")}</h2>
            <div class="saved">
              <p class="figure">
                {t("saved.figure", { time: minutes(now().saved_ms), days: String(now().days) })}
              </p>
              <p class="status">
                {t("saved.how", {
                  words: count(now().words),
                  typing: minutes((now().words / now().words_per_minute) * 60_000),
                  speed: String(now().words_per_minute),
                  spoken: minutes(now().spoken_ms),
                })}
              </p>
              <label class="speed">
                <span>{t("saved.speed")}</span>
                <input
                  type="number"
                  min="1"
                  max="250"
                  inputmode="numeric"
                  value={now().words_per_minute}
                  onChange={(event) => void chooseTypingSpeed(event.currentTarget.value)}
                />
                <span class="why">{t("saved.speed_unit")}</span>
              </label>
            </div>
            <p class="status">{t("saved.what")}</p>
          </>
        )}
      </Show>
    </div>
  );
}

export default Settings;
