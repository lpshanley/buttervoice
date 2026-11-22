import { createSignal, For, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
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
  | "model_unusable"
  | "language_not_supported"
  | "key_from_environment"
  | "key_saved"
  | "no_key";

type Setup = { provider: Provider; state: State };

const PROVIDERS: Provider[] = ["local", "grok"];

type LanguageSetup = { language: Locale; chosen: boolean };

// Each language is named in itself, so a person can find theirs whatever
// language the app happens to be showing.
const LANGUAGES: { tag: Locale; name: string }[] = [
  { tag: "en", name: "English" },
  { tag: "pt-BR", name: "Português (Brasil)" },
];

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

  const [setup, setSetup] = createSignal<Setup>({ provider: "local", state: "starting" });
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
    void recheck();
    // macOS does not say when a permission changes. Coming back to this
    // window from System Settings is the moment to look again.
    const watching = getCurrentWindow().onFocusChanged((event) => {
      if (event.payload) void recheck();
    });
    return () => {
      void watching.then((stop) => stop());
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
