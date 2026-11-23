/* @refresh reload */
import { createEffect } from "solid-js";
import { render } from "@solidjs/web";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Hud from "./Hud";
import { followTheApp, locale } from "./i18n";
import Samples from "./Samples";
import Settings from "./Settings";

// Each window loads this page and uses its label to choose the view.
const view = getCurrentWindow().label;
document.documentElement.dataset.view = view;
void followTheApp();

render(
  () => {
    createEffect(locale, (language) => {
      document.documentElement.lang = language;
    });
    return view === "settings" ? <Settings /> : view === "samples" ? <Samples /> : <Hud />;
  },
  document.getElementById("root") as HTMLElement,
);
