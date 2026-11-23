import { createEffect, createSignal, For, onSettled, Show, useContext } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Check from "./Check";
import { type Locale, t } from "./i18n";
import { type Provider, ReadinessContext } from "./readiness";

function megabytes(bytes: number) {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

const PROVIDERS: Provider[] = ["local", "grok"];

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

/** How dictation hears you: the language, who transcribes, and the microphone. */
function Dictation() {
  const readiness = useContext(ReadinessContext);
  const setup = readiness.transcription;
  const language = readiness.language;

  const [downloaded, setDownloaded] = createSignal(0);
  const [models, setModels] = createSignal<LocalModel[]>([]);
  const [draft, setDraft] = createSignal("");
  const [problem, setProblem] = createSignal("");

  async function refreshModels() {
    setModels(await invoke<LocalModel[]>("local_models"));
  }

  async function attempt(command: string, args?: Record<string, unknown>) {
    try {
      await invoke(command, args);
      setDraft("");
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    await readiness.refreshTranscription();
  }

  async function chooseModel(model: LocalModel) {
    // Choosing the recommended model goes back to following the
    // recommendation, so a better one later is picked up by itself.
    await attempt("choose_local_model", { id: model.recommended ? null : model.id });
  }

  async function download() {
    setDownloaded(0);
    const settled = attempt("download_model");
    // Show the download as started without waiting for it to finish.
    await readiness.refreshTranscription();
    await settled;
  }

  async function chooseLanguage(tag: Locale) {
    await attempt("choose_language", { language: tag });
    await readiness.refreshLanguage();
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

  // Ready, downloading, or missing a model or key already shows in the
  // controls, so only the other states are worth a line.
  const statusLine = () => {
    const state = setup().state;
    return state === "starting" ||
      state === "model_unusable" ||
      state === "language_not_supported" ||
      state === "ready_untested" ||
      state === "key_from_environment" ||
      state === "key_saved"
      ? t(`status.${state}`)
      : "";
  };

  // Which model is in use and which are downloaded follow the transcription state.
  createEffect(setup, () => void refreshModels());

  onSettled(() => {
    void refreshMicrophones();
    // A headset may have been connected while the window was away.
    const watching = getCurrentWindow().onFocusChanged((event) => {
      if (event.payload) void refreshMicrophones();
    });
    const progress = listen<number>("model-download", (event) => setDownloaded(event.payload));
    return () => {
      void watching.then((stop) => stop());
      void progress.then((stop) => stop());
    };
  });

  return (
    <div class="settings">
      <h2 id="language">{t("language.heading")}</h2>
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
      <Show when={setup().provider === "local" && models().length > 0}>
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
                  <strong>
                    {model.name}
                    <Show when={model.downloaded}>
                      <Check label={t("model.downloaded")} />
                    </Show>
                  </strong>
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
      <Show when={statusLine()}>
        <p class="status" role="status">
          {statusLine()}
        </p>
      </Show>
      <Show when={setup().state === "model_missing" && setup().model_bytes}>
        <button type="button" onClick={() => void download()}>
          {t("download.button", { size: megabytes(setup().model_bytes ?? 0) })}
        </button>
      </Show>
      <Show when={setup().state === "downloading"}>
        <progress max={setup().model_bytes ?? 1} value={downloaded()} />
        <p class="status">
          {t("download.progress", {
            done: megabytes(downloaded()),
            size: megabytes(setup().model_bytes ?? 0),
          })}
        </p>
        <button type="button" class="quiet" onClick={() => void invoke("cancel_model_download")}>
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
              value={draft()}
              onInput={(event) => setDraft(event.currentTarget.value)}
            />
            <button type="submit" disabled={!draft().trim()}>
              {t("key.save")}
            </button>
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
    </div>
  );
}

export default Dictation;
