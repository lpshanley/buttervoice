import { createSignal, For, onSettled, Show } from "solid-js";
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

type Part = {
  kind: "same" | "missing" | "extra" | "changed";
  said: string | null;
  heard: string | null;
};

type Take = { heard: string | null; seconds: number; difference: Part[] };

type Kept = { said: string; seconds: number };

type State = {
  active: boolean;
  position: number;
  total: number;
  prompt: Prompt | null;
  take: Take | null;
  kept_here: Kept | null;
  kept: number;
};

const NOT_STARTED: State = {
  active: false,
  position: 0,
  total: 0,
  prompt: null,
  take: null,
  kept_here: null,
  kept: 0,
};

function Samples() {
  const [state, setState] = createSignal<State>(NOT_STARTED);
  const [speaker, setSpeaker] = createSignal("");
  const [said, setSaid] = createSignal("");
  const [saved, setSaved] = createSignal("");
  const [problem, setProblem] = createSignal("");
  const [recording, setRecording] = createSignal("");

  // Playback reads the take, or the kept sample, straight from the app
  // into memory: nothing is written to disk to play it.
  async function loadRecording(now: State) {
    if (recording()) URL.revokeObjectURL(recording());
    if (!now.take && !now.kept_here) {
      setRecording("");
      return;
    }
    const wav = await invoke<ArrayBuffer>("samples_audio");
    setRecording(URL.createObjectURL(new Blob([wav], { type: "audio/wav" })));
  }

  async function refresh() {
    const now = await invoke<State>("samples_state");
    setState(now);
    // The box holds the words as spoken. A model writes "9:30" for "nine
    // thirty", so its text only starts the box when there was no script.
    if (now.take) setSaid(now.prompt?.say ?? now.take.heard ?? "");
    await loadRecording(now);
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

  // With a take waiting, Next keeps it as the words in the box, which start
  // as the script, so a take that matches is one press.
  async function next() {
    const silent = state().prompt?.kind === "silence";
    await act("samples_next", { said: silent ? "" : said() });
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
          {" · "}
          {t("samples.kept_count", { kept: state().kept })}
        </p>
        <h2>{t(`samples.kind.${state().prompt?.kind ?? "read_sentence"}`)}</h2>
        <Show when={state().prompt?.instruction}>
          <p class="instruction">{state().prompt?.instruction}</p>
        </Show>
        <Show when={state().prompt?.say}>
          <p class="say">{state().prompt?.say}</p>
        </Show>
        <Show when={recording()}>
          <audio controls src={recording()} />
        </Show>
        <Show
          when={state().take}
          fallback={
            <Show
              when={state().kept_here}
              fallback={<p class="status">{t("samples.hold")}</p>}
            >
              <p class="status">
                {t("samples.kept_here")} <q>{state().kept_here?.said}</q>
              </p>
              <p class="status">{t("samples.replace_hint")}</p>
            </Show>
          }
        >
          <Show when={state().take?.difference.length}>
            <p class="status">{t("samples.difference")}</p>
            <Show
              when={state().take?.difference.some((part) => part.kind !== "same")}
              fallback={<p class="status">{t("samples.no_difference")}</p>}
            >
              <p class="difference">
                <For each={state().take?.difference ?? []}>
                  {(part) => (
                    <>
                      <Show when={part.kind !== "extra" && part.said}>
                        <span class={part.kind === "same" ? "same" : "missing"}>{part.said}</span>
                      </Show>
                      <Show when={part.kind !== "same" && part.kind !== "missing" && part.heard}>
                        <span class="heard">{part.heard}</span>
                      </Show>{" "}
                    </>
                  )}
                </For>
              </p>
              <p class="status">{t("samples.difference_hint")}</p>
            </Show>
          </Show>
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
            <Show when={state().take?.heard && !state().take?.difference.length}>
              <p class="status">
                {t("samples.heard")} <q>{state().take?.heard}</q>
              </p>
            </Show>
          </Show>
          <p class="status">{t("samples.record_over")}</p>
        </Show>
        <div class="row">
          <button
            type="button"
            class="quiet"
            disabled={state().position === 0}
            onClick={() => void act("samples_back")}
          >
            {t("samples.back")}
          </button>
          <button type="button" class="quiet" onClick={() => void act("samples_skip")}>
            {t("samples.skip")}
          </button>
          <button
            type="button"
            disabled={!state().take && !state().kept_here}
            onClick={() => void next()}
          >
            {t("samples.next")}
          </button>
        </div>
      </Show>
      <Show when={state().active && !state().prompt}>
        <p class="status" role="status">
          {t("samples.done")}
        </p>
        <button
          type="button"
          class="quiet"
          disabled={state().position === 0}
          onClick={() => void act("samples_back")}
        >
          {t("samples.back")}
        </button>
      </Show>
      <Show when={state().active}>
        <div class="row">
          <button type="button" disabled={state().kept === 0} onClick={() => void finish()}>
            {state().prompt ? t("samples.save_now") : t("samples.save")}
          </button>
          <button type="button" class="quiet" onClick={() => void act("samples_cancel")}>
            {t("samples.cancel")}
          </button>
        </div>
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
