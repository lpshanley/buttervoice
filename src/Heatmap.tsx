import { For, Show } from "solid-js";
import { count, dayName, localDay, monthName, weekdayName } from "./format";
import { t } from "./i18n";

export type Day = { dictations: number; words: number; spoken_ms: number };

const WEEKS = 26;
const LEVELS = 4;
/** A month's short name needs about this many columns. */
const MONTH_LABEL_WEEKS = 3;

type Cell = { day: string; date: Date; words: number; dictations: number; level: number };

/** Busier days get deeper butter. Levels split the days that had any
 * dictation into quarters, so one huge day does not wash out the rest. */
function levelsFor(words: number[]) {
  const busy = words.filter((value) => value > 0).sort((a, b) => a - b);
  const cuts = [1, 2, 3].map((quarter) => busy[Math.floor((busy.length * quarter) / LEVELS)] ?? 0);
  return (value: number) =>
    value === 0 ? 0 : 1 + cuts.filter((cut) => value > cut).length;
}

/** Columns are weeks, Sunday at the top, ending with the current week. */
function weeksEndingToday(days: Record<string, Day>): Cell[][] {
  const today = new Date();
  const start = new Date(today);
  start.setDate(today.getDate() - today.getDay() - (WEEKS - 1) * 7);
  const cells: Omit<Cell, "level">[] = [];
  for (const date = new Date(start); date <= today; date.setDate(date.getDate() + 1)) {
    const day = localDay(date);
    cells.push({
      day,
      date: new Date(date),
      words: days[day]?.words ?? 0,
      dictations: days[day]?.dictations ?? 0,
    });
  }
  const level = levelsFor(cells.map((cell) => cell.words));
  const weeks: Cell[][] = [];
  cells.forEach((cell, index) => {
    if (index % 7 === 0) weeks.push([]);
    weeks[weeks.length - 1].push({ ...cell, level: level(cell.words) });
  });
  return weeks;
}

function Heatmap(props: { days: Record<string, Day> }) {
  const weeks = () => weeksEndingToday(props.days);
  const total = () => weeks().flat().reduce((sum, cell) => sum + cell.words, 0);
  // A month is named above the first week that starts in it, unless the
  // name before it is too close to fit beside it.
  const monthsAbove = () => {
    let lastNamed = -MONTH_LABEL_WEEKS;
    return weeks().map((week, index) => {
      const before = weeks()[index - 1]?.[0].date;
      const starts = !before || before.getMonth() !== week[0].date.getMonth();
      const room = weeks()
        .slice(index + 1, index + MONTH_LABEL_WEEKS)
        .every((later) => later[0].date.getMonth() === week[0].date.getMonth());
      if (!starts || index - lastNamed < MONTH_LABEL_WEEKS || !room) return "";
      lastNamed = index;
      return monthName(week[0].date);
    });
  };

  return (
    <figure class="heatmap">
      <div class="grid" role="img" aria-label={t("home.heatmap_label", { words: count(total()) })}>
        <span />
        <For each={monthsAbove()}>{(name) => <span class="month">{name}</span>}</For>
        <For each={[0, 1, 2, 3, 4, 5, 6]}>
          {(row) => (
            <>
              <span class="weekday">{row % 2 ? weekdayName(row) : ""}</span>
              <For each={weeks()}>
                {(week) => (
                  <Show when={week[row]} fallback={<span />}>
                    {(cell) => (
                      <span
                        class={`cell level-${cell().level}`}
                        title={t(cell().words ? "home.cell" : "home.cell_none", {
                          day: dayName(cell().date),
                          words: count(cell().words),
                          dictations: count(cell().dictations),
                        })}
                      />
                    )}
                  </Show>
                )}
              </For>
            </>
          )}
        </For>
      </div>
      <figcaption>
        <span>{t("home.heatmap_total", { words: count(total()) })}</span>
        <span class="legend" aria-hidden="true">
          {t("home.less")}
          <For each={[0, 1, 2, 3, 4]}>{(level) => <span class={`cell level-${level}`} />}</For>
          {t("home.more")}
        </span>
      </figcaption>
    </figure>
  );
}

export default Heatmap;
