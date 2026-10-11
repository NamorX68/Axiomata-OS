/**
 * The keys of other platforms in the editor engine's vocabulary (`docs/plans/keys-platforms.md`, K-10).
 *
 * The engine (`editor/keymap.ts`, `editor/vi/`) speaks the Mac only: ⌘ is the primary key, ⌥ moves by words, ⌘←/→ by
 * line. It imports nothing from the app (D1), so the platform is dealt with here, at the edge: `EditorSurface` hands
 * every key through {@link editorKeyInput} before the engine sees it. On the Mac that is the identity.
 *
 * Off the Mac the press is translated, not renamed — Ctrl+← is a word on Windows and Linux, not a line:
 *
 * | pressed                 | the engine hears | effect              |
 * |-------------------------|------------------|---------------------|
 * | Ctrl+←/→                | ⌥←/→             | word left/right     |
 * | Ctrl+Backspace/Delete   | ⌥⌫ / ⌥⌦          | delete a word       |
 * | Ctrl+Home/End           | ⌘Home/End        | document start/end  |
 * | Ctrl+↑/↓                | (nothing)        | not an editor key   |
 * | Ctrl+Shift+[ / ]        | ⌥⌘[ / ⌥⌘]        | fold / unfold here  |
 * | Ctrl+<other>            | ⌘<other>         | the same shortcut   |
 * | Home/End, Alt+↑/↓       | as they are      | line start/end, move lines |
 *
 * Fold has its own pair because Ctrl+Alt+[ is what AltGr+8 sends on a German layout and must stay typing.
 *
 * While Vi is on, the Ctrl letters its grammar knows (`<C-r>`, `<C-v>`, …) stay Vi's; the app's command on such a
 * letter (copy, paste, select all, …) is its Ctrl+Shift form.
 */

import type { KeyInput } from "../editor/keymap";
import { VI_GRAMMAR } from "../editor/vi/parse";
import { chord, type KeyLike, type Platform } from "../core/keys";

/** The press as the surface reads it: a `KeyboardEvent` has all of it. */
export type EditorKeyEvent = KeyLike & { keyCode?: number };

/** The letters (and `[`) Vi binds to a Ctrl key, read from its grammar so the list cannot fall behind it. */
export const VI_CTRL_KEYS: ReadonlySet<string> = new Set(
  Object.values(VI_GRAMMAR)
    .flat()
    .map((token) => /^<C-(.)>$/.exec(String(token))?.[1])
    .filter((key): key is string => key !== undefined),
);

/** The keys whose Ctrl form means a unit of movement or deletion, and the Mac key the engine knows it by. */
const UNIT_KEYS: Readonly<Record<string, Pick<KeyInput, "alt" | "meta">>> = {
  ArrowLeft: { alt: true, meta: false },
  ArrowRight: { alt: true, meta: false },
  Backspace: { alt: true, meta: false },
  Delete: { alt: true, meta: false },
  Home: { alt: false, meta: true },
  End: { alt: false, meta: true },
};

/** What the engine is told for a press. `vi` says Vi owns its Ctrl keys right now. */
export function editorKeyInput(e: EditorKeyEvent, on: Platform, vi: boolean): KeyInput {
  const raw: KeyInput = {
    key: e.key,
    meta: e.metaKey,
    alt: e.altKey,
    shift: e.shiftKey,
    ctrl: e.ctrlKey,
    keyCode: e.keyCode,
    code: e.code,
  };
  if (on === "mac") return raw;

  const c = chord(e, on);
  // AltGr wrote a character: the engine must see plain typing.
  if (c.typing) return { ...raw, meta: false, alt: false, ctrl: false };
  // The Super/Windows key belongs to the window manager.
  if (e.metaKey) return { ...raw, key: "Unidentified", meta: false, ctrl: false };
  if (!e.ctrlKey) return raw;

  // Fold / unfold here: the physical bracket keys, whatever character Shift makes of them.
  if ((e.code === "BracketLeft" || e.code === "BracketRight") && e.shiftKey && !e.altKey) {
    return { ...raw, key: e.code === "BracketLeft" ? "[" : "]", ctrl: false, meta: true, alt: true, shift: false };
  }
  const unit = UNIT_KEYS[e.key];
  if (unit && !e.altKey) return { ...raw, ctrl: false, ...unit };
  // Ctrl+↑/↓ (scroll on Windows) has no editor meaning; ⌘↑/↓ would jump to the document's ends. With Alt they are
  // the add-cursor keys (⌥⌘↑/↓) and go on. Ctrl+Alt+←/→ is nothing on the Mac, and ⌘← would be the line start.
  if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !e.altKey) return raw;
  if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && e.altKey) return raw;
  if (vi && !e.altKey && VI_CTRL_KEYS.has(e.key.toLowerCase())) {
    // Plain Ctrl+<letter> is Vi's; Ctrl+Shift+<letter> is the app's shortcut on it, without the Shift it needed.
    return e.shiftKey ? { ...raw, ctrl: false, meta: true, shift: false } : raw;
  }
  return { ...raw, ctrl: false, meta: true };
}
