import { createSignal, For, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CopyButton from "./CopyButton";
import Heatmap, { type Day } from "./Heatmap";
import { moment } from "./format";
import { t } from "./i18n";

type Entry = { text: string; at: number; app: { name: string } | null };

const LATEST = 3;

function Home(props: { onShowHistory: () => void }) {
  const [days, setDays] = createSignal<Record<string, Day>>({});
  const [latest, setLatest] = createSignal<Entry[]>([]);
  const [lastThisRun, setLastThisRun] = createSignal<string | null>(null);

  async function refresh() {
    setDays(await invoke<Record<string, Day>>("usage_days"));
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
