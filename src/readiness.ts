import { createContext, createSignal } from "solid-js";
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
import type { Locale } from "./i18n";

export type Provider = "local" | "grok";

export type State =
  | "starting"
  | "ready"
  | "model_missing"
  | "downloading"
  | "model_unusable"
  | "language_not_supported"
  | "ready_untested"
  | "key_from_environment"
  | "key_saved"
  | "no_key";

export type Transcription = { provider: Provider; state: State; model_bytes: number | null };

/** A provider is ready to dictate in these states. */
const READY: State[] = ["ready", "ready_untested", "key_from_environment", "key_saved"];

type LanguageSetup = { language: Locale; chosen: boolean };

export type Permission = {
  id: "microphone" | "input_monitoring" | "accessibility";
  check: () => Promise<boolean>;
  request: () => Promise<unknown>;
};

export const PERMISSIONS: Permission[] = [
  { id: "microphone", check: checkMicrophonePermission, request: requestMicrophonePermission },
  {
    id: "input_monitoring",
    check: checkInputMonitoringPermission,
    request: requestInputMonitoringPermission,
  },
  {
    id: "accessibility",
    check: checkAccessibilityPermission,
    request: requestAccessibilityPermission,
  },
];

export type Step = "language" | "transcription" | "permissions";

const STILL_STARTING_RECHECK_MS = 500;

/** What must be in place before the first dictation, shared by the
 * sections that set it up and the checklist that tracks it. */
export function createReadiness() {
  const [allowed, setAllowed] = createSignal<boolean[] | null>(null);
  const [transcription, setTranscription] = createSignal<Transcription>({
    provider: "local",
    state: "starting",
    model_bytes: null,
  });
  const [language, setLanguage] = createSignal<LanguageSetup>({ language: "en", chosen: true });

  async function recheckPermissions() {
    const now = await Promise.all(PERMISSIONS.map((permission) => permission.check()));
    setAllowed(now);
    return now;
  }

  async function refreshTranscription() {
    const now = await invoke<Transcription>("transcription_setup");
    setTranscription(now);
    // The model loads in the background at launch; look again until it has.
    if (now.state === "starting") {
      setTimeout(() => void refreshTranscription(), STILL_STARTING_RECHECK_MS);
    }
  }

  async function refreshLanguage() {
    const now = await invoke<LanguageSetup>("language_setup");
    setLanguage(now);
    return now;
  }

  const steps = (): { id: Step; done: boolean }[] => [
    { id: "language", done: language().chosen },
    { id: "transcription", done: READY.includes(transcription().state) },
    { id: "permissions", done: allowed()?.every(Boolean) ?? false },
  ];

  const settingUp = () => allowed() !== null && steps().some((step) => !step.done);

  /** Starts watching; `onMissing` runs once if the language or a
   * permission is missing at launch. Returns the cleanup. */
  function watch(onMissing: () => void) {
    void refreshTranscription();
    void Promise.all([refreshLanguage(), recheckPermissions()]).then(([now, permissions]) => {
      if (!now.chosen || permissions.includes(false)) onMissing();
    });
    // macOS does not say when a permission changes. Coming back to this
    // window from System Settings is the moment to look again.
    const watching = getCurrentWindow().onFocusChanged((event) => {
      if (event.payload) void recheckPermissions();
    });
    return () => void watching.then((stop) => stop());
  }

  return {
    allowed,
    transcription,
    language,
    steps,
    settingUp,
    recheckPermissions,
    refreshTranscription,
    refreshLanguage,
    watch,
  };
}

export type Readiness = ReturnType<typeof createReadiness>;

export const ReadinessContext = createContext<Readiness>();
