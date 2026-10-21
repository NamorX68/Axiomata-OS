/**
 * The chat input's own keys as plain logic (`AssistantBar.svelte` acts on the result). ⌘N there starts a new
 * conversation; outside the input it stays what the view makes of it (the Studio's new note), so the key is only
 * read while the input has the focus. Off the Mac it is Ctrl+Shift+N, as every letter command of the app's
 * (`core/keyHints.ts`'s `OTHER_KEYS`): plain Ctrl+N belongs to Vi and the shells. ⇧⇥ switches Plan/Build, as in
 * Claude Code; the input gives up moving the focus back with it.
 */

import { chord, type KeyLike, type Platform } from "./keys";

/** Whether a press in the chat input asks for a new conversation. */
export function isNewSessionPress(e: KeyLike, on: Platform): boolean {
  const c = chord(e, on);
  if (!c.primary || c.alt || c.typing || c.key !== "n") return false;
  return on === "mac" ? !c.shift : c.shift;
}

/** Whether a press in the chat input switches between Plan and Build: Shift+Tab, nothing else held. */
export function isModeTogglePress(e: KeyLike): boolean {
  return e.key === "Tab" && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey;
}
