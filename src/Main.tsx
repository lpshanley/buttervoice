import { createSignal, For, onSettled } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import GheeMascot from "./GheeMascot";
import History from "./History";
import Home from "./Home";
import { t } from "./i18n";
import Settings, { SETUP_NEEDED } from "./Settings";
import "./settings.css";

type Section = "home" | "history" | "settings";

const SECTIONS: Section[] = ["home", "history", "settings"];

/** One window for everything. Every section stays mounted, so each keeps
 * listening while another is shown. */
function Main() {
  const [section, setSection] = createSignal<Section>("home");

  onSettled(() => {
    const asked = listen<Section>("show-section", (event) => setSection(event.payload));
    const toSettings = () => setSection("settings");
    window.addEventListener(SETUP_NEEDED, toSettings);
    return () => {
      void asked.then((stop) => stop());
      window.removeEventListener(SETUP_NEEDED, toSettings);
    };
  });

  return (
    <div class="main">
      <nav class="sidebar" aria-label={t("nav.label")}>
        <div class="brand">
          <GheeMascot size={32} />
        </div>
        <For each={SECTIONS}>
          {(id) => (
            <button
              type="button"
              class={{ current: section() === id }}
              aria-current={section() === id ? "page" : undefined}
              onClick={() => setSection(id)}
            >
              {t(`nav.${id}`)}
            </button>
          )}
        </For>
      </nav>
      <div class="content">
        <section hidden={section() !== "home"}>
          <Home />
        </section>
        <section hidden={section() !== "history"}>
          <History />
        </section>
        <section hidden={section() !== "settings"}>
          <Settings />
        </section>
      </div>
    </div>
  );
}

export default Main;
