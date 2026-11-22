import { createSignal } from "solid-js";
import { resolveTemplate, translator } from "@solid-primitives/i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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

export { locale };

// The Rust side decides the display language, so the windows and the menu
// bar item never disagree. Until it answers, the system language stands in.
export async function followTheApp() {
  await listen<Locale>("display-language", (event) => setLocale(event.payload));
  const now = await invoke<{ language: Locale }>("language_setup");
  setLocale(now.language);
}

export const t = translator(() => dictionaries[locale()], resolveTemplate);
