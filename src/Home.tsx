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

function Home(props: { onShowHistory: () => void; onShowSettings: () => void }) {
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

  onSettled(() => {
    void refresh();
    const finished = listen("dictation-finished", () => void refresh());
    return () => void finished.then((stop) => stop());
  });

  return (
    <div class="settings">
      <h2>{t("nav.home")}</h2>
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
            <p class="status">
              {t("home.saved_how", {
                days: String(now().days),
                words: count(now().words),
                speed: String(now().words_per_minute),
                typing: minutes((now().words / now().words_per_minute) * 60_000),
                spoken: minutes(now().spoken_ms),
              })}{" "}
              <button type="button" class="link" onClick={() => {
                  props.onShowSettings();
                  // Once Settings is showing, bring the field into view.
                  requestAnimationFrame(() =>
                    document.getElementById("typing-speed")?.scrollIntoView({ behavior: "smooth" }),
                  );
                }}>
                {t("home.change_speed")}
              </button>
            </p>
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
