/* @refresh reload */
import { render } from "@solidjs/web";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Hud from "./Hud";
import { followTheApp } from "./i18n";
import Samples from "./Samples";
import Settings from "./Settings";

// Both windows load this page; the window's label says which view it is.
const view = getCurrentWindow().label;
document.documentElement.dataset.view = view;
void followTheApp();

render(
  () => (view === "settings" ? <Settings /> : view === "samples" ? <Samples /> : <Hud />),
  document.getElementById("root") as HTMLElement,
);
