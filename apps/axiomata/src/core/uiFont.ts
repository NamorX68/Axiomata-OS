/**
 * The UI typeface (`docs/plans/ui-ux-round.md`, U3/G1): one of three bundled
 * variable fonts, chosen in Appearance and set as `data-ui-font` on `<html>`.
 * `themes/tokens.css` maps that attribute to `--ax-font-sans`; code and
 * terminal keep their own fonts. Bundled (OFL-1.1, `@fontsource-variable/*`)
 * so the app looks the same on every machine and offline.
 */

import { writable, type Writable } from "svelte/store";

export const UI_FONTS = [
  { id: "geist", label: "Geist" },
  { id: "inter", label: "Inter" },
  { id: "plex", label: "IBM Plex Sans" },
] as const;

export type UiFont = (typeof UI_FONTS)[number]["id"];

/** Geist is the default (G1): the most distinctive of the three. */
export const DEFAULT_UI_FONT: UiFont = "geist";

/** The setting, persisted in `dashboard.json` by `persist.ts`. */
export const uiFont: Writable<UiFont> = writable(DEFAULT_UI_FONT);

/** Whether a stored value names a bundled font. */
export function isUiFont(value: unknown): value is UiFont {
  return UI_FONTS.some((f) => f.id === value);
}

/** Puts the font on `<html>` whenever the setting changes; returns the unsubscribe. */
export function startUiFont(): () => void {
  return uiFont.subscribe((font) => {
    document.documentElement.dataset.uiFont = font;
  });
}
