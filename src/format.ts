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
