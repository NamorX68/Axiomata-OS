/**
 * The Studio's own keys as plain logic (`IdeView.svelte` acts on the result): which action a key press asks for, or
 * `null` for any other key. Written in the terms of `core/keys.ts`, so ⌘ on the Mac is Ctrl elsewhere.
 *
 * Off the Mac the letter commands take **Ctrl+Shift**, not plain Ctrl: Vi owns Ctrl+P/N/O/B/F (completion, jump back,
 * page back/forward) and a terminal owns every plain Ctrl key, and this handler hears the key before either
 * (`docs/plans/keys-platforms.md`, K-6). Digits and `\` have no such owner and stay on plain Ctrl.
 */

import { chord, shiftedForm, type KeyLike, type Platform } from "../core/keys";

export type StudioAction =
  | { kind: "cycle-tab"; step: 1 | -1 }
  | { kind: "split"; where: "right" | "bottom" }
  | { kind: "tab"; number: number }
  | { kind: "close-file" }
  | { kind: "new-note" }
  | { kind: "open-file" }
  | { kind: "toggle-tree" }
  | { kind: "quick-open" }
  | { kind: "search" };

/** The key's data an action is read from — a `KeyboardEvent` has all of it. */
export type StudioKeyEvent = KeyLike;

/** The letters whose command takes a Shift off the Mac, and the action each asks for. */
const LETTER_ACTIONS: ReadonlyMap<string, StudioAction> = new Map<string, StudioAction>([
  ["w", { kind: "close-file" }],
  ["n", { kind: "new-note" }],
  ["o", { kind: "open-file" }],
  ["b", { kind: "toggle-tree" }],
  ["p", { kind: "quick-open" }],
  ["f", { kind: "search" }],
]);

/**
 * The tab number a digit key stands for. Off the Mac `code` is read as well: Shift turns `key` into a symbol there
 * (Ctrl+Shift+1 in a terminal), while on a Mac Shift never makes a tab key and an AZERTY `&` must stay `&`.
 */
function tabNumber(e: StudioKeyEvent, on: Platform): number | null {
  const fromCode = on === "mac" ? undefined : /^Digit([1-9])$/.exec(e.code ?? "")?.[1];
  const digit = /^[1-9]$/.test(e.key) ? e.key : fromCode;
  return digit ? Number(digit) : null;
}

/** Which Studio action a key press asks for. `terminal` says the press happened in a terminal pane. */
export function studioKeyAction(e: StudioKeyEvent, on: Platform, terminal = false): StudioAction | null {
  // The editor's tab keys: Ctrl+Tab is the same everywhere and no shell asks for it.
  if (e.ctrlKey && !e.metaKey && !e.altKey && e.key === "Tab") {
    return { kind: "cycle-tab", step: e.shiftKey ? -1 : 1 };
  }
  const c = chord(e, on, terminal);
  if (!c.primary || c.alt || c.typing) return null;
  if (e.code === "Backslash" || e.key === "\\") {
    return { kind: "split", where: e.code === "Backslash" && c.shift ? "bottom" : "right" };
  }
  const shifted = on === "mac" ? c.shift : shiftedForm(c, terminal);
  const number = tabNumber(e, on);
  // A digit is never in the Shift form on the Mac; off it, the terminal's Ctrl+Shift stands for Ctrl.
  if (number !== null && (on === "mac" ? !c.shift : terminal || !c.shift)) return { kind: "tab", number };
  // Shift can change `key` on some layouts; the physical F still means find.
  const action = LETTER_ACTIONS.get(e.code === "KeyF" ? "f" : c.key);
  if (!action) return null;
  // The Mac takes ⇧⌘F and plain ⌘ for the rest; elsewhere every letter command is Ctrl+Shift.
  const wantsShift = on === "mac" ? action.kind === "search" : true;
  return shifted === wantsShift ? action : null;
}
