import { createSignal, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./settings.css";

type KeySource = "environment" | "saved" | "none";

const STATUS: Record<KeySource, string> = {
  environment: "Using the key from the XAI_API_KEY environment variable.",
  saved: "A key is saved in your Keychain.",
  none: "No key yet. Dictations cannot be transcribed without one.",
};

function Settings() {
  const [source, setSource] = createSignal<KeySource>("none");
  const [draft, setDraft] = createSignal("");
  const [problem, setProblem] = createSignal("");

  async function refresh() {
    setSource(await invoke<KeySource>("api_key_source"));
  }

  async function attempt(command: string, args?: Record<string, unknown>) {
    try {
      await invoke(command, args);
      setDraft("");
      setProblem("");
    } catch (error) {
      setProblem(String(error));
    }
    await refresh();
  }

  onSettled(() => {
    void refresh();
  });

  return (
    <main class="settings">
      <h1>ButterVoice</h1>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void attempt("save_api_key", { key: draft() });
        }}
      >
        <label for="api-key">xAI API key</label>
        <div class="row">
          <input
            id="api-key"
            type="password"
            autocomplete="off"
            spellcheck="false"
            placeholder="Paste your key"
            value={draft()}
            onInput={(event) => setDraft(event.currentTarget.value)}
          />
          <button type="submit">Save</button>
        </div>
      </form>
      <p class="status" role="status">
        {STATUS[source()]}
      </p>
      <Show when={source() === "saved"}>
        <button type="button" class="quiet" onClick={() => void attempt("forget_api_key")}>
          Forget the saved key
        </button>
      </Show>
      <Show when={problem()}>
        <p class="problem" role="alert">
          {problem()}
        </p>
      </Show>
    </main>
  );
}

export default Settings;
