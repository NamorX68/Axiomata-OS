/**
 * The built-in theme list and the one place that applies a theme.
 *
 * Every id here has a matching `themes/<id>.css` file imported in `main.ts`.
 * A user's custom `~/.axiomata/theme.css` (step 13) layers on top of whichever
 * built-in is active; it is not a theme of its own.
 */

import { activeTheme } from "./stores";

import { tSettings } from "../i18n/settings";
export interface ThemeInfo {
  id: string;
  label: string;
  /** One line for the picker / tooltip, in the current language. */
  blurb: () => string;
}

export const THEMES: readonly ThemeInfo[] = [
  { id: "graphite", label: "Graphite", blurb: () => tSettings("blurbGraphite") },
  { id: "paper", label: "Paper", blurb: () => tSettings("blurbPaper") },
  { id: "steampunk", label: "Steampunk", blurb: () => tSettings("blurbSteampunk") },
  { id: "forest", label: "Forest", blurb: () => tSettings("blurbForest") },
  { id: "ocean", label: "Ocean", blurb: () => tSettings("blurbOcean") },
  { id: "catppuccin-mocha", label: "Catppuccin Mocha", blurb: () => tSettings("blurbCatppuccinMocha") },
  { id: "catppuccin-macchiato", label: "Catppuccin Macchiato", blurb: () => tSettings("blurbCatppuccinMacchiato") },
  { id: "catppuccin-frappe", label: "Catppuccin Frappé", blurb: () => tSettings("blurbCatppuccinFrappe") },
  { id: "catppuccin-latte", label: "Catppuccin Latte", blurb: () => tSettings("blurbCatppuccinLatte") },
  { id: "catppuccin-espresso", label: "Catppuccin Espresso", blurb: () => tSettings("blurbCatppuccinEspresso") },
  { id: "tokyo-night", label: "Tokyo Night", blurb: () => tSettings("blurbTokyoNight") },
  { id: "github-dark", label: "GitHub Dark", blurb: () => tSettings("blurbGithubDark") },
  { id: "github-light", label: "GitHub Light", blurb: () => tSettings("blurbGithubLight") },
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
