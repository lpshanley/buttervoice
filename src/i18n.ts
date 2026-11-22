import { createSignal } from "solid-js";
import { translator } from "@solid-primitives/i18n";
import english from "../locales/en.json";
import brazilianPortuguese from "../locales/pt-BR.json";

export type Locale = "en" | "pt-BR";

type Messages = typeof english;

// The two checks together make the type checker refuse a message that one
// language has and the other lacks, in either direction.
const dictionaries: Record<Locale, Messages> = {
  en: english satisfies typeof brazilianPortuguese,
  "pt-BR": brazilianPortuguese satisfies Messages,
};

function systemLocale(): Locale {
  return navigator.language.toLowerCase().startsWith("pt") ? "pt-BR" : "en";
}

const [locale, setLocale] = createSignal<Locale>(systemLocale());

export { locale, setLocale };

export const t = translator(() => dictionaries[locale()]);
