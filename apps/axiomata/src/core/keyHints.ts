/**
 * Shortcut text for people (`docs/plans/keys-platforms.md`, K-4). Labels, tooltips and the shortcut list are written
 * once in the Mac's glyphs (`⇧⌘F`) and shown in the platform's own words: unchanged on the Mac, `Ctrl+Shift+F`
 * elsewhere.
 *
 * Elsewhere the glyphs are turned into words mechanically, except where the key is not the same one: a command that
 * Vi or a shell would otherwise own takes Ctrl+Shift ({@link OTHER_KEYS}). That table is checked against the code that
 * matches the keys (`keyHints.test.ts`), so a label cannot name a key that does nothing.
 */

import type { Platform } from "./keys";

/** The keys that differ from a plain ⌘→Ctrl swap, by their Mac spelling. */
export const OTHER_KEYS: Readonly<Record<string, string>> = {
  "⌘P": "Ctrl+Shift+P",
  "⌘N": "Ctrl+Shift+N",
  "⌘O": "Ctrl+Shift+O",
  "⌘B": "Ctrl+Shift+B",
  "⌘W": "Ctrl+Shift+W",
  "⌘R": "Ctrl+Shift+R",
  "⌘⌫": "Ctrl+Shift+Backspace",
  "⌥⌘[": "Ctrl+Shift+[",
  "⌥⌘]": "Ctrl+Shift+]",
};

/** How Vi, when it is on, changes the keys shown. */
export interface HintOptions {
  /** Vi is on: its Ctrl letters are Vi's, so an app shortcut on one of them takes Ctrl+Shift. */
  vi?: boolean;
  /** The letters Vi binds; given by the caller so this module stays free of the editor. */
  viLetters?: ReadonlySet<string>;
}

const GLYPH_KEYS: Readonly<Record<string, string>> = {
  "⏎": "Enter",
  "↩": "Enter",
  "⌫": "Backspace",
  "⌦": "Delete",
  "⇥": "Tab",
  "⎋": "Esc",
};

/** A run of modifier glyphs and the key after it: `⇧⌘F`, `⌥⌘[`, `⌘Home`, `⌃Space`, `⌘F12`. */
const SPEC = /([⌃⌥⇧⌘]+)(Home|End|Space|PgUp|PgDn|F\d{1,2}|[^\s()])/g;

/** The words for a modifier run, in the order Windows and Linux write them. */
function modifierWords(glyphs: string): string[] {
  const words: string[] = [];
  if (glyphs.includes("⌘") || glyphs.includes("⌃")) words.push("Ctrl");
  if (glyphs.includes("⌥")) words.push("Alt");
  if (glyphs.includes("⇧")) words.push("Shift");
  return words;
}

/** One Mac spec (`⇧⌘F`) in the other platforms' words. */
export function otherNotation(spec: string, options: HintOptions = {}): string {
  const known = OTHER_KEYS[spec];
  if (known) return known;
  return spec.replace(SPEC, (_match, glyphs: string, key: string) => {
    const words = modifierWords(glyphs);
    const name = GLYPH_KEYS[key] ?? key;
    // A command on a Ctrl letter Vi binds takes Shift as well, unless it has one already.
    if (options.vi && options.viLetters?.has(name.toLowerCase()) && glyphs === "⌘") words.push("Shift");
    return [...words, name].join("+");
  });
}

/**
 * `text` with every shortcut written in it shown for the platform: `"Reload (⌘R)"` → `"Reload (Ctrl+Shift+R)"`.
 * The Mac gets its text back untouched.
 */
export function keyHints(text: string, on: Platform, options: HintOptions = {}): string {
  if (on === "mac") return text;
  // ⌘-click follows the primary key (K-2): it is Ctrl+click here, not a key called "-".
  const clicks = text.replace(/⌘-click/g, "Ctrl+click");
  const spelled = clicks.replace(SPEC, (spec) => otherNotation(spec, options));
  return spelled.replace(/[⏎↩⌫⌦⇥⎋]/g, (glyph) => GLYPH_KEYS[glyph]);
}
