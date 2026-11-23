import { For, Show, useContext } from "solid-js";
import Check from "./Check";
import { t } from "./i18n";
import { keyName } from "./hotkey";
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
              <Show when={!step.done} fallback={<Check label={t("setup.done")} />}>
                <button type="button" class="quiet" onClick={() => props.onGo(step.id)}>
                  {t("setup.show")}
                </button>
              </Show>
            </li>
          )}
        </For>
      </ol>
      <p class="status">{t("setup.then", { key: keyName() })}</p>
    </div>
  );
}

export default Setup;
