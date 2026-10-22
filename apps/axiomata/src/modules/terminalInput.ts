/**
 * Pure keyboard-to-byte mapping for the terminal module (Checkpoint 3
 * review follow-up), split out of `terminal.svelte`'s `handleKeydown` so it
 * is unit-testable. Since TUI0b (`docs/plans/terminal-tui.md`) it encodes
 * what a full-screen TUI tells apart: modifiers on every functional key,
 * F-keys, Alt, the cursor keys' application mode and the kitty keyboard
 * protocol's disambiguated presses.
 */

import { chord, type KeyLike, type Platform } from "../core/keys";

/** What a key press in the terminal is for: the shell, typing, a clipboard step, or the app. */
export type TerminalKeyRoute = "shell" | "typing" | "copy" | "paste" | "app";

/**
 * Who a key press in a terminal pane belongs to (`docs/plans/keys-platforms.md`, K-3, K-12). On the Mac nothing
 * changes: ⌘ never reaches a shell. Elsewhere every Ctrl combination is the shell's (Ctrl+C is SIGINT), except
 * Ctrl+Shift+C / Ctrl+Shift+V, which copy and paste as in other terminals, and every other Ctrl+Shift key, which is
 * the app's (`studioKeys.ts`, `globalKeys.ts`) and must not reach the shell as a control byte. AltGr is typing.
 */
export function terminalKeyRoute(e: KeyLike, on: Platform): TerminalKeyRoute {
  if (on === "mac") return "shell";
  const c = chord(e, on);
  if (c.typing) return "typing";
  if (!e.ctrlKey || e.metaKey || e.altKey || !e.shiftKey) return "shell";
  if (c.key === "c") return "copy";
  if (c.key === "v") return "paste";
  return "app";
}

/** What the running program asked the keyboard for (TUI0b), from the last screen event. */
export interface KeyMode {
  /** `?1`: cursor keys in application mode send `ESC O A` instead of `ESC [ A`. */
  appCursorKeys: boolean;
  /** The kitty keyboard protocol's flags; flag 1 (disambiguate) is the one the pane speaks. */
  kittyFlags: number;
}

/** No program asked for anything: what a shell's line editor expects. */
export const PLAIN_KEYS: KeyMode = { appCursorKeys: false, kittyFlags: 0 };

/** A key press as `encodeKey` reads it: the `KeyboardEvent` fields it needs. */
export type KeyPress = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">;

/** Kitty's "disambiguate escape codes" flag. */
const KITTY_DISAMBIGUATE = 1;

/** Cursor keys: their final byte, after `ESC [` (or `ESC O` in application mode), or `CSI 1 ; m` with modifiers. */
const CURSOR_KEYS: Readonly<Record<string, string>> = {
  ArrowUp: "A",
  ArrowDown: "B",
  ArrowRight: "C",
  ArrowLeft: "D",
  Home: "H",
  End: "F",
};

/** Editing keys and F5–F12: `CSI n ~`, `CSI n ; m ~` with modifiers. */
const TILDE_KEYS: Readonly<Record<string, number>> = {
  Insert: 2,
  Delete: 3,
  PageUp: 5,
  PageDown: 6,
  F5: 15,
  F6: 17,
  F7: 18,
  F8: 19,
  F9: 20,
  F10: 21,
  F11: 23,
  F12: 24,
};

/** F1–F4: `ESC O P` … `ESC O S`, `CSI 1 ; m P` with modifiers. */
const PF_KEYS: Readonly<Record<string, string>> = { F1: "P", F2: "Q", F3: "R", F4: "S" };

/** Ctrl with these characters gives the C0 control bytes beyond the letters: `Ctrl+[` is Escape, `Ctrl+Space` NUL. */
const CTRL_SYMBOLS: Readonly<Record<string, number>> = {
  " ": 0x00,
  "@": 0x00,
  "[": 0x1b,
  "\\": 0x1c,
  "]": 0x1d,
  "^": 0x1e,
  "_": 0x1f,
};

const encoder = new TextEncoder();
const bytes = (text: string) => encoder.encode(text);

/** xterm's modifier parameter: 1 plus Shift 1, Alt 2, Ctrl 4. */
function modifierParam(e: KeyPress): number {
  return 1 + (e.shiftKey ? 1 : 0) + (e.altKey ? 2 : 0) + (e.ctrlKey ? 4 : 0);
}

/** The character a letter or digit key gives without modifiers, from its physical position (`KeyA` → `a`), so a
 *  composed character can be told from a plain one. */
function baseCharacter(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1].toLowerCase();
  const digit = /^Digit([0-9])$/.exec(code);
  return digit ? digit[1] : null;
}

/**
 * The bytes a key press sends to the program, or `null` where the native `input` event types it (printable text, a
 * character Option composes, IME). Covers what a full-screen TUI tells apart (TUI0b): modifiers on cursor, editing and
 * F-keys in xterm's `CSI 1 ; m X` / `CSI n ; m ~` form, the cursor keys' application mode, F1–F12, Shift/Alt/Ctrl with
 * Enter, Tab and Backspace, Ctrl with letters and the C0 symbols, and the kitty keyboard protocol's disambiguated
 * `CSI code ; m u` for the presses the legacy encoding cannot tell apart.
 *
 * Option is Alt only where it composes nothing (D2 in `docs/plans/terminal-tui.md`): on a German Mac layout it types
 * `@ { } [ ] | \`, so Option+L stays `@`; Option+B (which gives `∫`) is not Alt either — only a key whose character
 * equals its plain one, and the functional keys. `typing` (AltGr elsewhere) leaves every character to `input`.
 */
export function encodeKey(e: KeyPress, mode: KeyMode, typing = false): Uint8Array | null {
  const m = modifierParam(e);
  const kitty = (mode.kittyFlags & KITTY_DISAMBIGUATE) !== 0;
  const ESC = "\x1b";
  const cursor = CURSOR_KEYS[e.key];
  if (cursor) {
    if (m > 1) return bytes(`${ESC}[1;${m}${cursor}`);
    return bytes(`${ESC}${mode.appCursorKeys ? "O" : "["}${cursor}`);
  }
  const tilde = TILDE_KEYS[e.key];
  if (tilde !== undefined) return bytes(m > 1 ? `${ESC}[${tilde};${m}~` : `${ESC}[${tilde}~`);
  const pf = PF_KEYS[e.key];
  if (pf) return bytes(m > 1 ? `${ESC}[1;${m}${pf}` : `${ESC}O${pf}`);
  switch (e.key) {
    case "Enter":
      if (kitty && m > 1) return bytes(`${ESC}[13;${m}u`);
      // Shift+Enter as Alt+Enter: what the owner's Ghostty sends and Claude Code and Glut read as a new line.
      if (e.altKey || e.shiftKey) return bytes(`${ESC}\r`);
      return bytes("\r");
    case "Tab":
      // `CSI Z` is Shift+Tab everywhere, also under the kitty protocol, where crossterm reads it as BackTab.
      if (e.shiftKey && !e.altKey && !(kitty && e.ctrlKey)) return bytes(`${ESC}[Z`);
      if (kitty && m > 1) return bytes(`${ESC}[9;${m}u`);
      return bytes(e.altKey ? `${ESC}\t` : "\t");
    case "Backspace":
      if (kitty && m > 1) return bytes(`${ESC}[127;${m}u`);
      if (e.ctrlKey) return new Uint8Array([0x08]);
      return e.altKey ? bytes(`${ESC}\x7f`) : new Uint8Array([0x7f]);
    case "Escape":
      if (kitty) return bytes(m > 1 ? `${ESC}[27;${m}u` : `${ESC}[27u`);
      return bytes(e.altKey ? `${ESC}${ESC}` : ESC);
  }
  if (e.key.length !== 1 || typing || e.metaKey) return null;
  const base = baseCharacter(e.code);
  // Option composes when the character differs from the key's plain one (`@` from L); a symbol key has no plain one
  // known here and is left to `input` too.
  const altAsAlt = e.altKey && base !== null && e.key.toLowerCase() === base;
  if (e.ctrlKey) {
    // The key's plain letter where it has one: with Option held too, WebKit may report a composed character.
    const letter = base !== null && base >= "a" && base <= "z" ? base : e.key.toLowerCase();
    if (kitty) {
      const code = (base ?? letter).codePointAt(0) ?? 0;
      return bytes(`${ESC}[${code};${m}u`);
    }
    const prefix = e.altKey ? ESC : "";
    if (letter >= "a" && letter <= "z") return bytes(prefix + String.fromCharCode(letter.charCodeAt(0) - 96));
    const symbol = CTRL_SYMBOLS[e.key];
    return symbol === undefined ? null : bytes(prefix + String.fromCharCode(symbol));
  }
  if (altAsAlt) {
    if (kitty) return bytes(`${ESC}[${(base ?? e.key).codePointAt(0) ?? 0};${m}u`);
    return bytes(ESC + e.key);
  }
  return null;
}
