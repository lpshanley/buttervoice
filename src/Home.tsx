import { createSignal, For, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CopyButton from "./CopyButton";
import { duration, moment } from "./format";
import { t } from "./i18n";

/** A dictation as the log recorded it: timings and outcome, never its text. */
type Recent = {
  id: number;
  at: string;
  release_to_text_ms: number | null;
  typing: "typed" | "failed" | "not_attempted";
};

function median(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

function Home() {
  const [last, setLast] = createSignal<string | null>(null);
  const [recent, setRecent] = createSignal<Recent[]>([]);

  async function refresh() {
    setLast(await invoke<string | null>("last_dictation"));
    setRecent(await invoke<Recent[]>("recent_dictations"));
  }

  onSettled(() => {
    void refresh();
    const finished = listen("dictation-finished", () => void refresh());
    return () => void finished.then((stop) => stop());
  });

  // Sample takes, blank takes and failed transcriptions were never to be
  // typed, so they say nothing about the wait.
  const attempted = () => recent().filter((dictation) => dictation.typing !== "not_attempted");
  const waits = () =>
    attempted()
      .map((dictation) => dictation.release_to_text_ms)
      .filter((wait): wait is number => wait !== null);
  const notTyped = () => attempted().filter((dictation) => dictation.typing === "failed").length;
  const oldestFirst = () => [...attempted()].reverse();
  const tallest = () => Math.max(...waits(), 1);

  return (
    <div class="settings">
      <h2>{t("nav.home")}</h2>
      <p class="how">{t("home.how")}</p>
      <h2>{t("home.last")}</h2>
      <Show when={last()} fallback={<p class="status">{t("home.none")}</p>}>
        <div class="dictation">
          <p class="text">{last()}</p>
          <CopyButton text={last() ?? ""} />
        </div>
      </Show>
      <h2>{t("home.recent")}</h2>
      <Show when={waits().length > 0} fallback={<p class="status">{t("home.no_recent")}</p>}>
        <dl class="figures">
          <div>
            <dt>{t("home.typical_wait")}</dt>
            <dd>{duration(median(waits()))}</dd>
          </div>
          <div>
            <dt>{t("home.slowest")}</dt>
            <dd>{duration(Math.max(...waits()))}</dd>
          </div>
          <div>
            <dt>{t("home.not_typed")}</dt>
            <dd>{notTyped()}</dd>
          </div>
        </dl>
        <ol class="waits" aria-label={t("home.chart_label")}>
          <For each={oldestFirst()}>
            {(dictation) => (
              <li
                class={{ not_typed: dictation.release_to_text_ms === null }}
                style={{
                  height: `${Math.max(((dictation.release_to_text_ms ?? 0) / tallest()) * 100, 4)}%`,
                }}
                title={
                  dictation.release_to_text_ms === null
                    ? t("home.bar_not_typed", { when: moment(new Date(dictation.at)) })
                    : t("home.bar", {
                        when: moment(new Date(dictation.at)),
                        wait: duration(dictation.release_to_text_ms),
                      })
                }
              />
            )}
          </For>
        </ol>
        <p class="status">{t("home.wait_what", { count: String(attempted().length) })}</p>
      </Show>
    </div>
  );
}

export default Home;
