import { For, Show, useContext } from "solid-js";
import { t } from "./i18n";
import { ReadinessContext, type Step } from "./readiness";

/** What is left before the first dictation. Each step leads to where it is done. */
function Setup(props: { onGo: (step: Step) => void }) {
  const readiness = useContext(ReadinessContext);
  return (
    <div class="settings">
      <h2>{t("setup.heading")}</h2>
      <ol class="steps">
        <For each={readiness.steps()}>
          {(step) => (
            <li class={{ done: step.done }}>
              <span>{t(`setup.${step.id}`)}</span>
              <Show when={!step.done} fallback={<span class="allowed">{t("setup.done")}</span>}>
                <button type="button" class="quiet" onClick={() => props.onGo(step.id)}>
                  {t("setup.show")}
                </button>
              </Show>
            </li>
          )}
        </For>
      </ol>
      <p class="status">{t("setup.then")}</p>
    </div>
  );
}

export default Setup;
