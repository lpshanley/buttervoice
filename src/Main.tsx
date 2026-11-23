import { createEffect, createSignal, For, onSettled } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Dictation from "./Dictation";
import General from "./General";
import GheeMascot from "./GheeMascot";
import History from "./History";
import Home from "./Home";
import { t } from "./i18n";
import { createReadiness, ReadinessContext, type Step } from "./readiness";
import Setup from "./Setup";
import UpdateNotice from "./UpdateNotice";
import "./settings.css";

type Section = "setup" | "home" | "history" | "dictation" | "general";

const ALWAYS: Section[] = ["home", "history", "dictation", "general"];

const STEP_SECTION: Record<Step, Section> = {
  language: "dictation",
  transcription: "dictation",
  permissions: "general",
};

/** One window for everything. Every section stays mounted, so each keeps
 * listening while another is shown. */
function Main() {
  const readiness = createReadiness();
  const [section, setSection] = createSignal<Section>("home");
  const sections = () => (readiness.settingUp() ? ["setup" as const, ...ALWAYS] : ALWAYS);

  function go(step: Step) {
    setSection(STEP_SECTION[step]);
    // Once the section is showing, bring the step into view.
    requestAnimationFrame(() =>
      document.getElementById(step)?.scrollIntoView({ behavior: "smooth" }),
    );
  }

  // Set up goes once everything is done, so nobody is left on it.
  createEffect(
    () => !readiness.settingUp() && section() === "setup",
    (finished) => {
      if (finished) setSection("home");
    },
  );

  onSettled(() => {
    const stopWatching = readiness.watch(() => {
      setSection("setup");
      const window = getCurrentWindow();
      void window.show().then(() => window.setFocus());
    });
    const asked = listen<Section>("show-section", (event) => setSection(event.payload));
    return () => {
      stopWatching();
      void asked.then((stop) => stop());
    };
  });

  return (
    <ReadinessContext value={readiness}>
      <div class="main">
        {/* The title bar is drawn over the window, so this strip moves it. */}
        <div class="drag" data-tauri-drag-region />
        <nav class="sidebar" aria-label={t("nav.label")}>
          <div class="brand">
            <GheeMascot size={32} />
          </div>
          <For each={sections()}>
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
          <UpdateNotice />
        </nav>
        <div class="content">
          <section hidden={section() !== "setup"}>
            <Setup onGo={go} />
          </section>
          <section hidden={section() !== "home"}>
            <Home onShowHistory={() => setSection("history")} />
          </section>
          <section hidden={section() !== "history"}>
            <History />
          </section>
          <section hidden={section() !== "dictation"}>
            <Dictation />
          </section>
          <section hidden={section() !== "general"}>
            <General />
          </section>
        </div>
      </div>
    </ReadinessContext>
  );
}

export default Main;
