/**
 * The app's window-wide keys as plain logic (`App.svelte` acts on the result): which action a key press asks for, or
 * `null` for any other key. The modifiers come from `core/keys.ts`: ⌘ on the Mac, Ctrl elsewhere.
 */

import { chord, isClosePress, type KeyLike, type Platform } from "./keys";

export type GlobalKeyAction = "spotlight" | "settings" | "close";

/** The key's data an action is read from — a `KeyboardEvent` has all of it. */
export type GlobalKeyEvent = KeyLike;

/**
 * ⌘K opens/closes the spotlight, ⌘, the settings, ⌘W closes the topmost window. Off the Mac the same with Ctrl, except
 * that closing is Ctrl+Shift+W: Ctrl+W is "delete word" in a shell and the terminal panes must keep it. In a
 * terminal pane every app key there takes Ctrl+Shift for the same reason. Shift and ⌥ otherwise make it another key.
 */
export function globalKeyAction(e: GlobalKeyEvent, on: Platform, terminal = false): GlobalKeyAction | null {
  if (isClosePress(e, on, terminal)) return "close";
  const c = chord(e, on, terminal);
  if (!c.primary || c.alt || c.typing || c.shift) return null;
  if (c.key === "k") return "spotlight";
  // `code` too: Shift (the terminal's Ctrl+Shift) turns `,` into `<` on a US layout.
  if (c.key === "," || e.code === "Comma") return "settings";
  return null;
}
