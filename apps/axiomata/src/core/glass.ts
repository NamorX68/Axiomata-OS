/**
 * How see-through the app's glass is (`docs/plans/glass.md`, G5, G12): one knob for every glass surface — panes,
 * panels, menus, dragged tiles — and macOS "Reduce transparency" on top of it.
 *
 * * **The knob** is the existing "Window transparency" setting (`windowTransparency` in `stores.ts`, 0 opaque – 100
 *   fully see-through, 50 each theme's own look). It becomes a multiplier on every theme's authored alpha, written to
 *   `--ax-tile-glass-opacity` (read by `--ax-glass-opacity` in `themes/tokens.css`): 0 at the see-through end, 1 in
 *   the middle, {@link SOLID_GLASS} at the solid end, where every alpha passes 1 and CSS clamps it.
 * * **"Reduce transparency"** wins over the knob, like "Reduce motion" over the motion setting: the glass turns
 *   solid and loses its blur (`data-transparency="reduced"` on `<html>`). WebKit's `prefers-reduced-transparency`
 *   cannot be relied on, so the Mac's own setting is asked (`reduce_transparency`); the media query still counts,
 *   for the browser check.
 *
 * The value is an inline style on `<html>`: the one place that beats every theme's `[data-theme]` rule and survives
 * a theme switch untouched (`applyTheme` never touches `documentElement.style`).
 */

import { derived, writable, type Readable, type Writable } from "svelte/store";

import { invokeBackend } from "./backend";
import { windowTransparency } from "./stores";

/**
 * The multiplier that makes every theme's glass solid: the thinnest authored alpha, a dark pane's 0.46
 * (`--ax-glass-pane-alpha`), times this passes 1.
 */
export const SOLID_GLASS = 2.2;

/** Whether the system asks for reduced transparency (kept current by {@link startGlass}). */
export const systemReducesTransparency: Writable<boolean> = writable(false);

/** The setting at which every theme looks as authored (multiplier 1). */
const THEME_LOOK = 50;

/**
 * The alpha multiplier in effect: `setting` is "how transparent" (0–100), the multiplier runs the other way —
 * from 0 up to 1 at {@link THEME_LOOK}, on to {@link SOLID_GLASS} at 0; the system's "Reduce transparency" makes it
 * solid whatever the setting says.
 */
export function glassOpacity(setting: number, systemReduces: boolean): number {
  if (systemReduces) return SOLID_GLASS;
  const clamped = Math.min(100, Math.max(0, Number.isFinite(setting) ? setting : THEME_LOOK));
  if (clamped >= THEME_LOOK) return (100 - clamped) / (100 - THEME_LOOK);
  return 1 + ((THEME_LOOK - clamped) / THEME_LOOK) * (SOLID_GLASS - 1);
}

/** The multiplier in effect, for anything that draws glass itself. */
export const glass: Readable<number> = derived([windowTransparency, systemReducesTransparency], ([setting, reduces]) =>
  glassOpacity(setting, reduces),
);

/**
 * Follows the system's "Reduce transparency" (asked at the start and whenever the window comes back to the front,
 * where a change in System Settings shows) and puts the glass in effect on `<html>`; returns a function that stops it.
 */
export function startGlass(): () => void {
  const query = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-transparency: reduce)") : null;
  let native = false;
  let stopped = false;
  const publish = (): void => systemReducesTransparency.set(native || (query?.matches ?? false));
  const ask = async (): Promise<void> => {
    try {
      native = await invokeBackend<boolean>("reduce_transparency");
    } catch {
      native = false;
    }
    if (!stopped) publish();
  };
  publish();
  void ask();
  query?.addEventListener("change", publish);
  const onFocus = (): void => void ask();
  window.addEventListener("focus", onFocus);

  const root = document.documentElement;
  const unsubscribeOpacity = glass.subscribe((opacity) => {
    root.style.setProperty("--ax-tile-glass-opacity", String(opacity));
  });
  const unsubscribeReduced = systemReducesTransparency.subscribe((reduces) => {
    if (reduces) root.dataset.transparency = "reduced";
    else delete root.dataset.transparency;
  });
  return () => {
    stopped = true;
    query?.removeEventListener("change", publish);
    window.removeEventListener("focus", onFocus);
    unsubscribeOpacity();
    unsubscribeReduced();
  };
}
