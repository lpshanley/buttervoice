import { createSignal, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";
import "./settings.css";

type Kind =
  | "read_sentence"
  | "short_utterance"
  | "long_dictation"
  | "hard_words"
  | "natural_speech"
  | "conditions"
  | "silence";

type Prompt = {
  kind: Kind;
  say: string | null;
  instruction: string | null;
  conditions: string | null;
};

type Take = { heard: string | null; seconds: number };

type State = {
  active: boolean;
  position: number;
  total: number;
  prompt: Prompt | null;
  take: Take | null;
};

const NOT_STARTED: State = { active: false, position: 0, total: 0, prompt: null, take: null };

function Samples() {
  const [state, setState] = createSignal<State>(NOT_STARTED);
  const [speaker, setSpeaker] = createSignal("");
  const [said, setSaid] = createSignal("");
  const [saved, setSaved] = createSignal("");
  const [problem, setProblem] = createSignal("");

  async function refresh() {
    const now = await invoke<State>("samples_state");
    setState(now);
    // What a model heard is only a starting point for the person to correct.
    if (now.take) setSaid(now.take.heard ?? now.prompt?.say ?? "");
  }

  async function act(command: string, args?: Record<string, unknown>) {
    try {
      await invoke(command, args);
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    await refresh();
  }

  async function start() {
    setSaved("");
    await act("samples_start", { speaker: speaker() });
  }

  async function keep() {
    const silent = state().prompt?.kind === "silence";
    await act("samples_keep", { said: silent ? "" : said() });
  }

  async function finish() {
    try {
      setSaved(await invoke<string>("samples_finish"));
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    await refresh();
  }

  onSettled(() => {
    void refresh();
    const watching = listen("samples-changed", () => void refresh());
    return () => {
      void watching.then((stop) => stop());
    };
  });

  return (
    <main class="settings samples">
      <h1>{t("samples.title")}</h1>
      <Show when={saved()}>
        <p class="status" role="status">
          {t("samples.saved")} <code>{saved()}</code>
        </p>
      </Show>
      <Show when={!state().active}>
        <p class="status">{t("samples.intro")}</p>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void start();
          }}
        >
          <label for="speaker">{t("samples.speaker_label")}</label>
          <div class="row">
            <input
              id="speaker"
              autocomplete="off"
              spellcheck="false"
              placeholder={t("samples.speaker_placeholder")}
              value={speaker()}
              onInput={(event) => setSpeaker(event.currentTarget.value)}
            />
            <button type="submit">{t("samples.start")}</button>
          </div>
        </form>
      </Show>
      <Show when={state().active && state().prompt}>
        <p class="status" role="status">
          {t("samples.progress", { position: state().position + 1, total: state().total })}
        </p>
        <h2>{t(`samples.kind.${state().prompt?.kind ?? "read_sentence"}`)}</h2>
        <Show when={state().prompt?.instruction}>
          <p class="status">{state().prompt?.instruction}</p>
        </Show>
        <Show when={state().prompt?.say}>
          <p class="say">{state().prompt?.say}</p>
        </Show>
        <Show when={state().take} fallback={<p class="status">{t("samples.hold")}</p>}>
          <Show
            when={state().prompt?.kind !== "silence"}
            fallback={<p class="status">{t("samples.nothing_said")}</p>}
          >
            <label for="said">{t("samples.said_label")}</label>
            <textarea
              id="said"
              rows="4"
              spellcheck="false"
              value={said()}
              onInput={(event) => setSaid(event.currentTarget.value)}
            />
            <p class="status">{t("samples.said_hint")}</p>
          </Show>
          <div class="row">
            <button type="button" onClick={() => void keep()}>
              {t("samples.keep")}
            </button>
            <button type="button" class="quiet" onClick={() => void act("samples_redo")}>
              {t("samples.redo")}
            </button>
          </div>
        </Show>
        <button type="button" class="quiet" onClick={() => void act("samples_skip")}>
          {t("samples.skip")}
        </button>
      </Show>
      <Show when={state().active && !state().prompt}>
        <p class="status" role="status">
          {t("samples.done")}
        </p>
        <button type="button" onClick={() => void finish()}>
          {t("samples.save")}
        </button>
      </Show>
      <Show when={state().active}>
        <button type="button" class="quiet" onClick={() => void act("samples_cancel")}>
          {t("samples.cancel")}
        </button>
      </Show>
      <Show when={problem()}>
        <p class="problem" role="alert">
          {problem()}
        </p>
      </Show>
    </main>
  );
}

export default Samples;
