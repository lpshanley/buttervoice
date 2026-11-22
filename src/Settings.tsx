import { createSignal, For, onSettled, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  checkAccessibilityPermission,
  checkInputMonitoringPermission,
  checkMicrophonePermission,
  requestAccessibilityPermission,
  requestInputMonitoringPermission,
  requestMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import "./settings.css";

type KeySource = "environment" | "saved" | "none";

const STATUS: Record<KeySource, string> = {
  environment: "Using the key from the XAI_API_KEY environment variable.",
  saved: "A key is saved in your Keychain.",
  none: "No key yet. Dictations cannot be transcribed without one.",
};

type Permission = {
  name: string;
  why: string;
  check: () => Promise<boolean>;
  request: () => Promise<unknown>;
};

const PERMISSIONS: Permission[] = [
  {
    name: "Microphone",
    why: "To hear what you say.",
    check: checkMicrophonePermission,
    request: requestMicrophonePermission,
  },
  {
    name: "Input Monitoring",
    why: "To notice the dictation key.",
    check: checkInputMonitoringPermission,
    request: requestInputMonitoringPermission,
  },
  {
    name: "Accessibility",
    why: "To type the text into other apps.",
    check: checkAccessibilityPermission,
    request: requestAccessibilityPermission,
  },
];

function Settings() {
  const [allowed, setAllowed] = createSignal<boolean[] | null>(null);
  let checkedBefore = false;

  async function recheck() {
    const now = await Promise.all(PERMISSIONS.map((permission) => permission.check()));
    const first = !checkedBefore;
    checkedBefore = true;
    setAllowed(now);
    // Something is missing at launch: bring the window up to say so.
    if (first && now.includes(false)) {
      const window = getCurrentWindow();
      await window.show();
      await window.setFocus();
    }
  }

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
    void recheck();
    // macOS does not say when a permission changes. Coming back to this
    // window from System Settings is the moment to look again.
    const watching = getCurrentWindow().onFocusChanged((event) => {
      if (event.payload) void recheck();
    });
    return () => {
      void watching.then((stop) => stop());
    };
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
      <h2>Permissions</h2>
      <ul class="permissions">
        <For each={PERMISSIONS}>
          {(permission, index) => (
            <li>
              <div>
                <strong>{permission.name}</strong>
                <span class="why">{permission.why}</span>
              </div>
              <Show
                when={allowed()?.[index()] === false}
                fallback={<span class="allowed">{allowed() ? "Allowed" : ""}</span>}
              >
                <button
                  type="button"
                  onClick={() => void permission.request().then(() => recheck())}
                >
                  Allow…
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
    </main>
  );
}

export default Settings;
