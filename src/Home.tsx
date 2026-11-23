import { createSignal, For, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CopyButton from "./CopyButton";
import Heatmap, { type Day } from "./Heatmap";
import { count, minutes, moment } from "./format";
import { t } from "./i18n";

type Entry = { text: string; at: number; app: { name: string } | null };

const LATEST = 3;

type TimeSaved = {
  days: number;
  words_per_minute: number;
  dictations: number;
  words: number;
  spoken_ms: number;
  saved_ms: number;
};

function Home(props: { onShowHistory: () => void }) {
  const [saved, setSaved] = createSignal<TimeSaved | null>(null);
  const [days, setDays] = createSignal<Record<string, Day>>({});
  const [latest, setLatest] = createSignal<Entry[]>([]);
  const [lastThisRun, setLastThisRun] = createSignal<string | null>(null);

  async function refresh() {
    setDays(await invoke<Record<string, Day>>("usage_days"));
    setSaved(await invoke<TimeSaved>("time_saved"));
    setLatest((await invoke<Entry[]>("history_search", { query: "" })).slice(0, LATEST));
    // With the history off, this run's last dictation is all there is.
    setLastThisRun(await invoke<string | null>("last_dictation"));
  }

  async function chooseTypingSpeed(text: string) {
    const wordsPerMinute = Math.round(Number(text));
    try {
      if (wordsPerMinute > 0) await invoke("choose_typing_speed", { wordsPerMinute });
    } catch {
      // Refused: reading it back below shows the speed still saved.
    }
    setSaved(await invoke<TimeSaved>("time_saved"));
  }

  onSettled(() => {
    void refresh();
    const finished = listen("dictation-finished", () => void refresh());
    return () => void finished.then((stop) => stop());
  });

  return (
    <div class="settings">
      <p class="how">{t("home.how")}</p>
      <Show when={saved()}>
        {(now) => (
          <>
            <dl class="figures">
              <div>
                <dt>{t("home.time_saved")}</dt>
                <dd>{minutes(now().saved_ms)}</dd>
              </div>
              <div>
                <dt>{t("home.words")}</dt>
                <dd>{count(now().words)}</dd>
              </div>
              <div>
                <dt>{t("home.dictations")}</dt>
                <dd>{count(now().dictations)}</dd>
              </div>
            </dl>
            <label class="status speed">
              {t("home.period", { days: String(now().days) })}
              {" · "}
              <input
                type="number"
                min="1"
                max="250"
                inputmode="numeric"
                aria-label={t("speed.heading")}
                value={now().words_per_minute}
                onChange={(event) => void chooseTypingSpeed(event.currentTarget.value)}
              />
              {t("speed.unit")}
            </label>
          </>
        )}
      </Show>
      <Heatmap days={days()} />
      <h2>{t("home.latest")}</h2>
      <Show
        when={latest().length > 0}
        fallback={
          <Show when={lastThisRun()} fallback={<p class="status">{t("home.none")}</p>}>
            <ul class="history">
              <li>
                <p class="text">{lastThisRun()}</p>
                <CopyButton text={lastThisRun() ?? ""} />
              </li>
            </ul>
          </Show>
        }
      >
        <ul class="history">
          <For each={latest()}>
            {(entry) => (
              <li>
                <div>
                  <p class="text">{entry.text}</p>
                  <span class="why">
                    {[moment(new Date(entry.at * 1000)), entry.app?.name].filter(Boolean).join(" · ")}
                  </span>
                </div>
                <CopyButton text={entry.text} />
              </li>
            )}
          </For>
        </ul>
        <button type="button" class="quiet" onClick={() => props.onShowHistory()}>
          {t("home.see_all")}
        </button>
      </Show>
    </div>
  );
}

export default Home;
