import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";

export const HOTKEYS = [
  "right_option",
  "left_option",
  "right_command",
  "right_control",
  "left_control",
  "fn",
] as const;

export type Hotkey = (typeof HOTKEYS)[number];

/** What each key's tile shows: the symbol printed on the key, and which side it is on. */
export const KEY_CAPS: Record<Hotkey, { symbol: string; side: "left" | "right" | null }> = {
  right_option: { symbol: "⌥", side: "right" },
  left_option: { symbol: "⌥", side: "left" },
  right_command: { symbol: "⌘", side: "right" },
  right_control: { symbol: "⌃", side: "right" },
  left_control: { symbol: "⌃", side: "left" },
  fn: { symbol: "fn", side: null },
};

const [hotkey, setHotkey] = createSignal<Hotkey>("right_option");

export { hotkey };

/** The chosen key's name, as the windows' instructions say it. */
export const keyName = () => t(`hotkey.${hotkey()}`);

// The Rust side holds the choice, so every window names the same key.
export async function followTheHotkey() {
  await listen<Hotkey>("hotkey", (event) => setHotkey(event.payload));
  setHotkey(await invoke<Hotkey>("hotkey_setup"));
}

export async function chooseHotkey(key: Hotkey) {
  await invoke("choose_hotkey", { key });
}
