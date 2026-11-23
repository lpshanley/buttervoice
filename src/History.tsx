import { createSignal, For, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CopyButton from "./CopyButton";
import { moment } from "./format";
import { t } from "./i18n";

type Entry = {
  text: string;
  /** Seconds since 1970. */
  at: number;
  language: string | null;
  backend: string | null;
  app: { name: string; bundle_id: string | null } | null;
};

function History() {
  const [query, setQuery] = createSignal("");
  const [found, setFound] = createSignal<Entry[]>([]);
  const [keep, setKeep] = createSignal(true);
  const [confirming, setConfirming] = createSignal(false);
  const [problem, setProblem] = createSignal("");
  // Answers can arrive out of order while typing; only the latest counts.
  let asked = 0;

  async function search() {
    const question = ++asked;
    const answer = await invoke<Entry[]>("history_search", { query: query() });
    if (question === asked) setFound(answer);
  }

  async function chooseKeep(on: boolean) {
    try {
      await invoke("choose_keep_history", { on });
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    setKeep(await invoke<boolean>("history_setup"));
  }

  async function clear() {
    try {
      await invoke("clear_history");
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    setConfirming(false);
    await search();
  }

  onSettled(() => {
    void search();
    void invoke<boolean>("history_setup").then(setKeep);
    const finished = listen("dictation-finished", () => void search());
    return () => void finished.then((stop) => stop());
  });

  return (
    <div class="settings">
      <h2>{t("nav.history")}</h2>
      <input
        type="search"
        class="search"
        placeholder={t("history.search")}
        aria-label={t("history.search")}
        spellcheck="false"
        value={query()}
        onInput={(event) => {
          setQuery(event.currentTarget.value);
          void search();
        }}
      />
      <Show when={!keep()}>
        <p class="status" role="status">
          {t("history.off")}
        </p>
      </Show>
      <Show
        when={found().length > 0}
        fallback={
          <p class="status">{query().trim() ? t("history.none_found") : t("history.empty")}</p>
        }
      >
        <ul class="history">
          <For each={found()}>
            {(entry) => (
              <li>
                <div>
                  <p class="text">{entry.text}</p>
                  <span class="why">
                    {[moment(new Date(entry.at * 1000)), entry.app?.name, entry.language]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </div>
                <CopyButton text={entry.text} />
              </li>
            )}
          </For>
        </ul>
      </Show>
      <label class="switch">
        <input
          type="checkbox"
          checked={keep()}
          onChange={(event) => void chooseKeep(event.currentTarget.checked)}
        />
        <span>{t("history.keep")}</span>
      </label>
      <p class="status">{t("history.keep_what")}</p>
      <Show
        when={confirming()}
        fallback={
          <button type="button" class="quiet" onClick={() => setConfirming(true)}>
            {t("history.clear")}
          </button>
        }
      >
        <div class="confirm" role="alertdialog" aria-label={t("history.clear")}>
          <p>{t("history.clear_confirm")}</p>
          <div class="row">
            <button type="button" class="danger" onClick={() => void clear()}>
              {t("history.clear_yes")}
            </button>
            <button type="button" class="quiet" onClick={() => setConfirming(false)}>
              {t("history.clear_no")}
            </button>
          </div>
        </div>
      </Show>
      <Show when={problem()}>
        <p class="problem" role="alert">
          {problem()}
        </p>
      </Show>
    </div>
  );
}

export default History;
