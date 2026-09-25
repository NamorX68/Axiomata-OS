/**
 * Keys as the Vi machine sees them (`docs/plans/editor.md`, ED3, V5, V12).
 *
 * A key is one of three things:
 *
 * * **a string** — one character (`"d"`, `"ä"`, `"^"`), or a special key in
 *   Vim's own notation: `"<Esc>"`, `"<CR>"`, `"<BS>"`, `"<C-r>"`, `"<Left>"` …
 *   The view reads characters, not key codes, so every keyboard layout works
 *   (V12);
 * * **`{ text }`** — a run of typed text in Insert mode (an input method's
 *   composition, a paste), inserted as it is, never read as commands;
 * * **`{ command }`** — a normal-mode (Mac) command the view already resolved
 *   in Insert mode (⌥⌫, ⌘←, ⇧→ …, V5); run as it is, and recorded for `.`.
 *
 * Macros live in registers as text, so keys also go to and from Vim's notation
 * (`keysToText` / `parseKeys`): `ciw<Esc>` is five keys, `<lt>` a literal `<`.
 */

import type { Command } from "../commands";

export type ViKey = string | { text: string } | { command: Command };

/** Special keys the notation knows, by their name inside `<…>`. */
const SPECIAL = new Set([
  "Esc",
  "CR",
  "BS",
  "Del",
  "Tab",
  "S-Tab",
  "Space",
  "Left",
  "Right",
  "Up",
  "Down",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "lt",
]);

/** `<C-x>` for any single letter or `[`. */
const CTRL = /^C-([a-z[\]])$/;

function isSpecialName(name: string): boolean {
  return SPECIAL.has(name) || CTRL.test(name);
}

/** Keys from Vim notation: `"d2w"`, `"ciwfoo<Esc>"`, `"<C-r>a"`. `<lt>` is a literal `<`. */
export function parseKeys(notation: string): ViKey[] {
  const keys: ViKey[] = [];
  const chars = [...notation];
  for (let i = 0; i < chars.length; i++) {
    if (chars[i] === "<") {
      const close = chars.indexOf(">", i + 1);
      const name = close > i ? chars.slice(i + 1, close).join("") : "";
      if (close > i && isSpecialName(name)) {
        keys.push(name === "lt" ? "<" : name === "Space" ? " " : `<${name}>`);
        i = close;
        continue;
      }
    }
    keys.push(chars[i]);
  }
  return keys;
}

/**
 * Keys as Vim notation, for a macro register. Typed text becomes its
 * characters; a resolved Mac command has no notation and is dropped — a macro
 * records what Vi keys did, not ⌥⌫ in Insert mode.
 */
export function keysToText(keys: readonly ViKey[]): string {
  let out = "";
  for (const key of keys) {
    if (typeof key === "string") out += key === "<" ? "<lt>" : key;
    else if ("text" in key) out += key.text.replace(/</g, "<lt>");
  }
  return out;
}

/** Whether `key` is the single character `ch`. */
export function isChar(key: ViKey | undefined, ch?: string): key is string {
  if (typeof key !== "string" || key.length === 0) return false;
  if (key.startsWith("<") && key.length > 1) return false;
  return ch === undefined || key === ch;
}

/** A key's single character, or `null` for a special key, text or command. */
export function charOf(key: ViKey | undefined): string | null {
  return isChar(key) ? key : null;
}

/** Keys that end Insert mode (and cancel a pending command in Normal mode). */
export function isEscape(key: ViKey | undefined): boolean {
  return key === "<Esc>" || key === "<C-[>" || key === "<C-c>";
}
