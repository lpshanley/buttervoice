import { type Locale, locale } from "./i18n";

/** Building an `Intl` formatter is slow and lists call these per row, so
 * each language builds its own once. */
function perLocale<Format>(make: (locale: Locale) => Format) {
  const made = new Map<Locale, Format>();
  return () => {
    const now = locale();
    let format = made.get(now);
    if (!format) {
      format = make(now);
      made.set(now, format);
    }
    return format;
  };
}

const tenths = perLocale((now) => new Intl.NumberFormat(now, { maximumFractionDigits: 1 }));
const whole = perLocale((now) => new Intl.NumberFormat(now));
const dateAndTime = perLocale(
  (now) => new Intl.DateTimeFormat(now, { dateStyle: "medium", timeStyle: "short" }),
);
const date = perLocale((now) => new Intl.DateTimeFormat(now, { dateStyle: "medium" }));
const month = perLocale((now) => new Intl.DateTimeFormat(now, { month: "short" }));
const weekday = perLocale((now) => new Intl.DateTimeFormat(now, { weekday: "short" }));

/** Under a second in milliseconds, otherwise in seconds to one decimal. */
export function duration(ms: number) {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${tenths().format(ms / 1000)} s`;
}

export function moment(when: Date) {
  return dateAndTime().format(when);
}

export function dayName(when: Date) {
  return date().format(when);
}

export function monthName(when: Date) {
  return month().format(when);
}

/** 0 is Sunday. */
export function weekdayName(index: number) {
  // 1 November 2026 was a Sunday.
  return weekday().format(new Date(2026, 10, 1 + index));
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
  return whole().format(value);
}
