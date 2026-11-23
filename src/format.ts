import { locale } from "./i18n";

/** Under a second in milliseconds, otherwise in seconds to one decimal. */
export function duration(ms: number) {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const seconds = new Intl.NumberFormat(locale(), { maximumFractionDigits: 1 }).format(ms / 1000);
  return `${seconds} s`;
}

export function moment(when: Date) {
  return new Intl.DateTimeFormat(locale(), { dateStyle: "medium", timeStyle: "short" }).format(
    when,
  );
}

/** Whole minutes, with hours once there are enough: `1 h 13 min`. */
export function minutes(ms: number) {
  const total = Math.round(ms / 60_000);
  const hours = Math.floor(total / 60);
  return hours ? `${count(hours)} h ${total % 60} min` : `${total} min`;
}

/** The date on this Mac's calendar, as the tally keys it: `2026-11-22`. */
export function localDay(when: Date) {
  const month = String(when.getMonth() + 1).padStart(2, "0");
  const day = String(when.getDate()).padStart(2, "0");
  return `${when.getFullYear()}-${month}-${day}`;
}

export function count(value: number) {
  return new Intl.NumberFormat(locale()).format(value);
}
