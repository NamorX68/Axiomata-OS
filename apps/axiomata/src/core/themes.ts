/**
 * The built-in theme list and the one place that applies a theme.
 *
 * Every id here has a matching `themes/<id>.css` file imported in `main.ts`.
 * A user's custom `~/.axiomata/theme.css` (step 13) layers on top of whichever
 * built-in is active; it is not a theme of its own.
 */

import { activeTheme } from "./stores";

export interface ThemeInfo {
  id: string;
  label: string;
  /** One line for the picker / tooltip. */
  blurb: string;
}

export const THEMES: readonly ThemeInfo[] = [
  { id: "graphite", label: "Graphite", blurb: "Near-black, orange accent, hex mesh" },
  { id: "paper", label: "Paper", blurb: "Warm cream, ink text, clay accent" },
  { id: "steampunk", label: "Steampunk", blurb: "Bronze and leather, brass accent" },
  { id: "forest", label: "Forest", blurb: "Pine and moss, autumn-amber accent" },
  { id: "ocean", label: "Ocean", blurb: "Deep-sea navy, coral accent" },
];

export const DEFAULT_THEME = THEMES[0].id;

export function isTheme(id: string): boolean {
  return THEMES.some((t) => t.id === id);
}

/** Set `<html data-theme>` and the store. Unknown ids fall back to the default. */
export function applyTheme(id: string): void {
  const next = isTheme(id) ? id : DEFAULT_THEME;
  document.documentElement.dataset.theme = next;
  activeTheme.set(next);
}

/** The theme after `current` in `THEMES`, wrapping around. */
export function nextTheme(current: string): string {
  const idx = THEMES.findIndex((t) => t.id === current);
  return THEMES[(idx + 1) % THEMES.length].id;
}
