/**
 * The app's window-wide keys as plain logic (`App.svelte` acts on the result): which action a key press asks for, or
 * `null` for any other key. Written for the Mac (⌘); elsewhere Ctrl stands in only for the keys a terminal has no use for.
 */

export type GlobalKeyAction = "spotlight" | "settings" | "close";

/** The key's data an action is read from — a `KeyboardEvent` has all of it. */
export type GlobalKeyEvent = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/**
 * ⌘K opens/closes the spotlight, ⌘, the settings, ⌘W closes the topmost window. Ctrl is the modifier only off the Mac,
 * and never for ⌘W: Ctrl+W is "delete word" in a shell and the terminal panes must keep it. Shift and ⌥ always make it
 * another key.
 */
export function globalKeyAction(e: GlobalKeyEvent, isMac: boolean): GlobalKeyAction | null {
  const modifier = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (!modifier || e.altKey || e.shiftKey) return null;
  switch (e.key.toLowerCase()) {
    case "k":
      return "spotlight";
    case ",":
      return "settings";
    case "w":
      return isMac ? "close" : null;
    default:
      return null;
  }
}
