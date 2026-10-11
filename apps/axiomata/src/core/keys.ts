/**
 * The key layer (`docs/plans/keys-platforms.md`, card #71): which platform's keys the app speaks, and what a key press
 * means in that platform's terms. Every shortcut asks {@link chord} for its modifiers instead of reading `metaKey` or
 * `ctrlKey` itself, so the Mac's ⌘ and the other platforms' Ctrl are decided in one place.
 *
 * * **Primary key**: ⌘ on the Mac, Ctrl elsewhere.
 * * **Terminal**: a terminal pane hands every Ctrl combination to the shell, so off the Mac an app shortcut there is
 *   Ctrl+Shift+<key> (`terminal: true`). The Mac is unaffected: ⌘ never reaches a shell.
 * * **AltGr is typing**: on a German layout under Windows and Linux AltGr arrives as Ctrl+Alt and writes
 *   `@ € \ [ ] { }`; such a press is never a shortcut.
 * * **Simulation**: the "Keyboard" setting forces a platform, so the whole layer can be exercised on the Mac.
 */

import { derived, get, writable, type Readable, type Writable } from "svelte/store";

/** The two key conventions the app knows. Windows and Linux share one. */
export type Platform = "mac" | "other";

/** The "Keyboard" setting: follow the machine, or pretend to be one of the platforms. */
export type KeyboardSetting = "auto" | Platform;

/** What the setting offers, in the order of the select. */
export const KEYBOARD_SETTINGS: readonly { value: KeyboardSetting; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "mac", label: "Mac" },
  { value: "other", label: "Windows & Linux" },
];

/** The part of a `KeyboardEvent` the layer reads; `getModifierState` is optional so tests can hand a plain object. */
export type KeyLike = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"> &
  Partial<Pick<KeyboardEvent, "code">> & { getModifierState?: (key: string) => boolean };

/** A key press in the terms of the current platform. */
export interface Chord {
  /** ⌘ on the Mac, Ctrl elsewhere (Ctrl+Shift in a terminal), and nothing else held down besides Alt/Shift. */
  primary: boolean;
  /** ⌥ on the Mac, Alt elsewhere. */
  alt: boolean;
  /** Shift, not counting the one a terminal's Ctrl+Shift stands for. */
  shift: boolean;
  /** The press writes a character (AltGr): it is no shortcut, whatever else the flags say. */
  typing: boolean;
  /** `key` in lower case, for comparing. */
  key: string;
}

/** The words for a platform in the settings: the one place they are spelled. */
export function platformLabel(on: Platform): string {
  return KEYBOARD_SETTINGS.find((option) => option.value === on)?.label ?? on;
}

/** The "Keyboard" setting, persisted in `dashboard.json` by `persist.ts`. */
export const keyboardSetting: Writable<KeyboardSetting> = writable("auto");

/** Whether a stored value is a valid setting. */
export function isKeyboardSetting(value: unknown): value is KeyboardSetting {
  return value === "auto" || value === "mac" || value === "other";
}

/** The machine's own platform, from the browser's report. */
export function detectPlatform(nav: Pick<Navigator, "platform"> | undefined = globalThis.navigator): Platform {
  return nav?.platform?.toLowerCase().includes("mac") ? "mac" : "other";
}

/** The platform a setting stands for on this machine. */
export function resolvePlatform(setting: KeyboardSetting, detected: Platform): Platform {
  return setting === "auto" ? detected : setting;
}

/** The platform in effect: what every shortcut and every label follows. */
export const platform: Readable<Platform> = derived(keyboardSetting, (setting) =>
  resolvePlatform(setting, detectPlatform()),
);

/** The platform in effect right now, for code outside a component. */
export function currentPlatform(): Platform {
  return get(platform);
}

/** A character that is not a letter or digit: what AltGr produces where a shortcut would name an ASCII letter. */
const NON_ALPHANUMERIC = /^[^a-z0-9]$/i;

/**
 * Whether the press is AltGr. Chromium reports it through `getModifierState`; where a webview does not (WebKitGTK is
 * unchecked), Ctrl+Alt that produced a character which is no ASCII letter or digit is taken for it: no shortcut of the
 * app is Ctrl+Alt+<symbol>.
 */
function altGraph(e: KeyLike): boolean {
  if (e.getModifierState?.("AltGraph") === true) return true;
  return e.ctrlKey && e.altKey && !e.metaKey && e.key.length === 1 && NON_ALPHANUMERIC.test(e.key);
}

/**
 * Reads a key press for a platform. `terminal` says the press happened in a terminal pane (only matters off the Mac).
 */
export function chord(e: KeyLike, on: Platform, terminal = false): Chord {
  const key = e.key.toLowerCase();
  if (on === "mac") {
    return { primary: e.metaKey && !e.ctrlKey, alt: e.altKey, shift: e.shiftKey, typing: false, key };
  }
  if (altGraph(e)) return { primary: false, alt: false, shift: false, typing: true, key };
  // The Windows/Super key is the window manager's; a press with it held is not ours.
  const base = e.ctrlKey && !e.metaKey;
  if (terminal) return { primary: base && e.shiftKey, alt: e.altKey, shift: false, typing: false, key };
  return { primary: base, alt: e.altKey, shift: e.shiftKey, typing: false, key };
}

/**
 * Whether a press is in its "Shift form" off the Mac: Shift is the key's own outside a terminal, and inside one the
 * Ctrl+Shift that `chord` has already taken stands for it (the shell owns plain Ctrl). The Mac has no such form.
 */
export function shiftedForm(c: Chord, terminal: boolean): boolean {
  return terminal || c.shift;
}

/**
 * Whether a press closes the window it happens in: ⌘W on the Mac, Ctrl+Shift+W elsewhere (plain Ctrl+W is "delete
 * word" for a shell and Vi's window prefix).
 */
export function isClosePress(e: KeyLike, on: Platform, terminal = false): boolean {
  const c = chord(e, on, terminal);
  if (!c.primary || c.alt || c.typing || c.key !== "w") return false;
  return on === "mac" ? !c.shift : shiftedForm(c, terminal);
}

/**
 * Whether the primary key is held on a pointer event, for ⌘-click / Ctrl+click (go to definition, open a link).
 * Mouse events carry no `key`, so this is {@link chord}'s primary test on the two modifiers alone.
 */
export function primaryHeld(e: Pick<MouseEvent, "metaKey" | "ctrlKey">, on: Platform): boolean {
  return on === "mac" ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

/**
 * Whether the *other* platform's system modifier is held (Ctrl on the Mac, ⌘/Super elsewhere): a press that is no
 * shortcut of ours.
 */
export function foreignModifier(e: Pick<KeyLike, "metaKey" | "ctrlKey">, on: Platform): boolean {
  return on === "mac" ? e.ctrlKey : e.metaKey;
}

/** The class of the terminal pane's element: what {@link inTerminalPane} looks for. */
const TERMINAL_SELECTOR = ".terminal";

/** Whether a key event happened inside a terminal pane, where every plain Ctrl key belongs to the shell. */
export function inTerminalPane(e: { target: EventTarget | null }): boolean {
  return !!(e.target as Element | null)?.closest?.(TERMINAL_SELECTOR);
}
