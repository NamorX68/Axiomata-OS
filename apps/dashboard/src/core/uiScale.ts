/**
 * The UI scale (`docs/plans/editor-look.md`, LK0, K9–K11, K13): one factor,
 * `--ax-ui-scale` on `<html>`, that every UI font size, spacing, radius, icon
 * and hit target in `themes/tokens.css` is multiplied by. Content fonts — the
 * editor's, the terminal's — are not.
 *
 * * **"Auto"** takes the scale Rust worked out for the display the window is
 *   on, from its real density (`ui_displays`, `axiomata-macos::display`).
 * * **A percentage** (the "UI size" setting) overrides it everywhere.
 * * **The canvas keeps its units**: tile positions and sizes are stored
 *   unscaled and drawn times the scale ({@link uiScale}), so a tile grows
 *   with its text instead of cutting it off.
 * * **The window's display is followed**: its position is looked at once a
 *   second (the webview has no "moved" event), and the displays are asked
 *   again only when it left the one it was on, or the app gets the focus back
 *   (a monitor may have been plugged in meanwhile).
 */

import { get, writable, type Writable } from "svelte/store";

import { invokeBackend } from "./backend";

/** A display as `ui_displays` reports it: bounds in points of the global display space. */
export interface UiDisplay {
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

/** "Auto", or a fixed size in percent. */
export type UiSize = "auto" | number;

/** The sizes the setting offers besides "Auto". */
export const UI_SIZES = [90, 100, 110, 120, 125, 130, 140, 150] as const;

/** The "UI size" setting (K13), persisted in `dashboard.json` by `persist.ts`. */
export const uiSize: Writable<UiSize> = writable("auto");

/**
 * The scale in effect: what `--ax-ui-scale` is set to. The canvas works in
 * unscaled units and multiplies by this when it draws (tiles, guides).
 */
export const uiScale: Writable<number> = writable(1);

/** What "Auto" comes to on the window's display now, for the settings to show. */
export const uiScaleAuto: Writable<number> = writable(1);

/** How often the window's position is looked at. */
const FOLLOW_MS = 1000;

/** Whether a stored value is a valid setting. */
export function isUiSize(value: unknown): value is UiSize {
  return value === "auto" || (typeof value === "number" && (UI_SIZES as readonly number[]).includes(value));
}

/**
 * The display a window centred at (`x`, `y`) is on: the one containing that
 * point, else the nearest one, else `null` (no displays known).
 */
export function displayAt(displays: readonly UiDisplay[], x: number, y: number): UiDisplay | null {
  let best: UiDisplay | null = null;
  let bestDistance = Infinity;
  for (const d of displays) {
    const dx = Math.max(d.x - x, 0, x - (d.x + d.width));
    const dy = Math.max(d.y - y, 0, y - (d.y + d.height));
    const distance = dx * dx + dy * dy;
    if (distance < bestDistance) {
      best = d;
      bestDistance = distance;
    }
  }
  return best;
}

/** The scale for a setting: a percentage as it is, "Auto" as the display asks. */
export function scaleFor(size: UiSize, display: UiDisplay | null): number {
  return size === "auto" ? (display?.scale ?? 1) : size / 100;
}

function contains(d: UiDisplay, x: number, y: number): boolean {
  return x >= d.x && x < d.x + d.width && y >= d.y && y < d.y + d.height;
}

/** Starts following the setting and the window's display; returns the function that stops it. */
export function startUiScale(): () => void {
  let displays: UiDisplay[] = [];
  let current: UiDisplay | null = null;
  let stopped = false;

  const centre = () => ({ x: window.screenX + window.outerWidth / 2, y: window.screenY + window.outerHeight / 2 });
  const apply = () => {
    const scale = scaleFor(get(uiSize), current);
    document.documentElement.style.setProperty("--ax-ui-scale", String(scale));
    uiScale.set(scale);
    uiScaleAuto.set(current?.scale ?? 1);
  };
  const refresh = async () => {
    try {
      displays = await invokeBackend<UiDisplay[]>("ui_displays");
    } catch {
      displays = [];
    }
    if (stopped) return;
    const { x, y } = centre();
    current = displayAt(displays, x, y);
    apply();
  };
  const follow = () => {
    const { x, y } = centre();
    if (!current || !contains(current, x, y)) void refresh();
  };

  const unsubscribe = uiSize.subscribe(apply);
  const timer = setInterval(follow, FOLLOW_MS);
  const onFocus = () => void refresh();
  window.addEventListener("focus", onFocus);
  void refresh();
  return () => {
    stopped = true;
    unsubscribe();
    clearInterval(timer);
    window.removeEventListener("focus", onFocus);
  };
}
