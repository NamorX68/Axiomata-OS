/**
 * Which key presses go where in Vi mode (`docs/plans/editor.md`, ED3, V5, V12).
 *
 * * **Characters come through the textarea, not the key code** (V12): a
 *   printable key is left alone on `keydown`, and its character arrives as
 *   input — so `[` typed as ⌥5 on a German keyboard is `[`, whatever the key.
 * * **Special keys and Ctrl** become Vi keys here (`<Esc>`, `<CR>`, `<C-r>` …).
 * * **⌘ keeps its Mac meaning** (V5): save, open, the preview and ⌥Z as in the
 *   normal map; ⌘Z/⌘⇧Z undo and redo; ⌘C/⌘X/⌘V copy, cut and paste through the
 *   Mac clipboard (`"+`); ⌘A selects everything; ⌘/ comments.
 * * **In Insert mode the whole Mac map applies** (V5): ⌥⌫, ⌘←, ⇧→ … are
 *   resolved to commands and handed to the machine, which runs and records them.
 * * **On the command line** (`:`, `/`, `?`, ED3.3) characters are typed text
 *   again, special keys edit the line, ⌘V pastes into it.
 */

import { keyAction, type Effect, type KeyInput } from "../editor/keymap";
import type { ViKey } from "../editor/vi/keys";
import type { ViMode } from "../editor/vi/machine";

export type ViKeyDecision =
  /** One key for the machine. */
  | { kind: "key"; key: ViKey }
  /** Keys in Vim notation, fed in order (⌘Z → `u`). */
  | { kind: "keys"; notation: string }
  /** What the surface does itself, as in the normal map (save, open, wrap, preview; copy/cut/paste in Insert). */
  | { kind: "effect"; effect: Effect }
  /** Let the browser deliver it as typed text. */
  | { kind: "text" }
  /** Not the editor's key. */
  | null;

const SPECIAL: Record<string, string> = {
  Escape: "<Esc>",
  Enter: "<CR>",
  Backspace: "<BS>",
  Delete: "<Del>",
  ArrowLeft: "<Left>",
  ArrowRight: "<Right>",
  ArrowUp: "<Up>",
  ArrowDown: "<Down>",
  Home: "<Home>",
  End: "<End>",
  PageUp: "<PageUp>",
  PageDown: "<PageDown>",
};

function isInsert(mode: ViMode): boolean {
  return mode === "insert" || mode === "replace";
}

function isVisual(mode: ViMode): boolean {
  return mode === "visual" || mode === "visualLine" || mode === "visualBlock";
}

/** ⌘ shortcuts outside Insert mode, as Vi keys (V5). */
function commandKeys(key: string, shift: boolean, mode: ViMode): string | null {
  const visual = isVisual(mode);
  switch (key) {
    case "z":
      return shift ? "<C-r>" : "u";
    case "c":
      return visual ? '"+y' : '"+yy';
    case "x":
      return visual ? '"+d' : '"+dd';
    case "v":
      return shift ? null : '"+P';
    case "a":
      return visual ? "<Esc>ggVG" : "ggVG";
    case "/":
      return visual ? "gc" : "gcc";
  }
  return null;
}

/** A key on the command line: text, the line's own editing keys, the Mac's word/line keys, ⌘V. */
function commandLineKey(input: KeyInput, mac: ReturnType<typeof keyAction>): ViKeyDecision {
  const { key, meta, alt, shift } = input;
  if (key === "Escape") return { kind: "key", key: "<Esc>" };
  if (!meta && !alt && !shift && SPECIAL[key]) return { kind: "key", key: SPECIAL[key] };
  if (key === "Tab" && !meta && !alt) return { kind: "key", key: shift ? "<S-Tab>" : "<Tab>" };
  if (mac && "command" in mac) return { kind: "key", key: { command: mac.command } };
  if (mac && "effect" in mac && mac.effect === "paste") return { kind: "effect", effect: "paste" };
  if (meta) return null;
  return key.length === 1 || key === "Dead" ? { kind: "text" } : null;
}

/** What a key press means in Vi mode `mode`; `cmdline` while the command line is open. */
export function viKeyFor(input: KeyInput, mode: ViMode, cmdline = false): ViKeyDecision {
  const { key, meta, alt, shift, ctrl } = input;
  const insert = isInsert(mode);

  if (ctrl && !meta) {
    if (key === "[") return { kind: "key", key: "<C-[>" };
    if (/^[a-zA-Z]$/.test(key)) return { kind: "key", key: `<C-${key.toLowerCase()}>` };
    return null;
  }

  // Effects the normal map knows (⌘S ⌘O ⌘⇧V ⌥Z) keep their meaning everywhere.
  const mac = keyAction(input);
  if (mac && "effect" in mac && ["save", "open", "togglePreview", "toggleWrap"].includes(mac.effect)) {
    return { kind: "effect", effect: mac.effect };
  }

  if (cmdline) return commandLineKey(input, mac);

  if (insert) {
    if (key === "Escape") return { kind: "key", key: "<Esc>" };
    // Plain special keys are Vi's own in Insert mode (they are recorded as keys for `.`).
    if (!meta && !alt && !shift && SPECIAL[key]) return { kind: "key", key: SPECIAL[key] };
    if (key === "Tab" && !meta && !alt) return { kind: "key", key: shift ? "<S-Tab>" : "<Tab>" };
    if (mac && "command" in mac) return { kind: "key", key: { command: mac.command } };
    if (mac && "effect" in mac) return { kind: "effect", effect: mac.effect };
    return key.length === 1 || key === "Dead" ? { kind: "text" } : null;
  }

  if (meta && !ctrl) {
    const notation = commandKeys(key.length === 1 ? key.toLowerCase() : key, shift, mode);
    return notation ? { kind: "keys", notation } : null;
  }
  if (SPECIAL[key]) return { kind: "key", key: SPECIAL[key] };
  if (key === "Tab") return { kind: "key", key: "<Tab>" };
  // Characters — ⌥-typed ones included — come through the textarea (V12).
  return key.length === 1 || key === "Dead" || alt ? { kind: "text" } : null;
}
