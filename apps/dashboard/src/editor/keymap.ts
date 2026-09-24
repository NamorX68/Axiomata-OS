/**
 * The normal (Mac) key map (`docs/plans/editor.md`, ED1, F5; D16 for who owns
 * which key).
 *
 * A pure function from a key press to what it means: a model `Command`, an
 * `Effect` the view has to carry out (clipboard, saving, opening, wrapping), or
 * `null` — then the key is not the editor's, and a printable one reaches the
 * hidden textarea as ordinary typing.
 *
 * Letters are read from `key`, not `code`: on a German QWERTZ keyboard ⌘Z is the
 * key labelled Z, which `code` calls `KeyY`. ⌥ turns letters into other
 * characters on a Mac (⌥Z types `Ω`), so ⌥ shortcuts name that character.
 */

import type { Command, Motion } from "./commands";

export interface KeyInput {
  key: string;
  meta: boolean;
  alt: boolean;
  shift: boolean;
  ctrl: boolean;
  /**
   * The legacy `KeyboardEvent.keyCode`. On a Mac WebKit reports the layout's
   * unmodified letter there even when ⌥ turns the key into a dead key (`key`
   * is then just "Dead") — the one way to recognise an ⌥ shortcut on every
   * layout. Optional: only ⌥ shortcuts look at it.
   */
  keyCode?: number;
}

/** `keyCode` of the Z key, whatever the layout puts on it with ⌥. */
const KEYCODE_Z = 90;

export type Effect = "copy" | "cut" | "paste" | "save" | "open" | "toggleWrap" | "togglePreview";

export type KeyAction = { command: Command } | { effect: Effect };

function motion(m: Motion, extend: boolean): KeyAction {
  return { command: { type: "move", motion: m, extend } };
}

function command(c: Command): KeyAction {
  return { command: c };
}

/** What `input` means to the editor, or `null` if it is not the editor's key. */
export function keyAction(input: KeyInput): KeyAction | null {
  const { meta, alt, shift, ctrl } = input;
  if (ctrl) return null;
  // ⌥Z types "Ω" on a US Mac layout, "z" off the Mac, and on some layouts it is a
  // dead key ("Dead", e.g. ¨) — `keyCode` catches that last case. Checked before
  // lowercasing, since "Ω".toLowerCase() is "ω".
  const isZ = input.key === "Ω" || input.key === "z" || input.key === "Z" || input.keyCode === KEYCODE_Z;
  if (alt && !meta && isZ) return { effect: "toggleWrap" };
  const key = input.key.length === 1 ? input.key.toLowerCase() : input.key;

  switch (key) {
    case "ArrowLeft":
      return motion(meta ? "lineStart" : alt ? "wordLeft" : "charLeft", shift);
    case "ArrowRight":
      return motion(meta ? "lineEnd" : alt ? "wordRight" : "charRight", shift);
    case "ArrowUp":
      if (alt && !meta) return command(shift ? { type: "duplicateLines", dir: -1 } : { type: "moveLines", dir: -1 });
      return motion(meta ? "docStart" : "up", shift);
    case "ArrowDown":
      if (alt && !meta) return command(shift ? { type: "duplicateLines", dir: 1 } : { type: "moveLines", dir: 1 });
      return motion(meta ? "docEnd" : "down", shift);
    case "Home":
      return motion(meta ? "docStart" : "lineStart", shift);
    case "End":
      return motion(meta ? "docEnd" : "lineEnd", shift);
    case "PageUp":
      return motion("pageUp", shift);
    case "PageDown":
      return motion("pageDown", shift);
    case "Backspace":
      return command({ type: "deleteBackward", unit: meta ? "line" : alt ? "word" : "char" });
    case "Delete":
      return command({ type: "deleteForward", unit: alt ? "word" : "char" });
    case "Tab":
      return meta || alt ? null : command({ type: shift ? "outdent" : "indent" });
    case "Enter":
      return meta || alt ? null : command({ type: "newline" });
  }

  if (!meta || alt) return null;
  switch (key) {
    case "a":
      return command({ type: "selectAll" });
    case "z":
      return command({ type: shift ? "redo" : "undo" });
    case "x":
      return { effect: "cut" };
    case "c":
      return { effect: "copy" };
    case "v":
      // ⌘⇧V cycles the Markdown preview (G8); plain ⌘V pastes.
      return { effect: shift ? "togglePreview" : "paste" };
    case "s":
      return { effect: "save" };
    case "o":
      return { effect: "open" };
    case "l":
      return command({ type: "selectLine" });
    // ⌘/ on a US keyboard, ⌘⇧7 on a German one — both report "/".
    case "/":
      return command({ type: "toggleComment" });
  }
  return null;
}
