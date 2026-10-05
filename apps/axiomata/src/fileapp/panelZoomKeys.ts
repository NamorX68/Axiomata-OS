/**
 * The floating file window's zoom keys (⌘+, ⌘-, ⌘0) as plain logic, so the mapping from a key press to a size
 * step is tested without a DOM. The steps themselves are `editorSettings.ts`'s `panelFontSize*` functions.
 */

import { panelFontSizeDown, panelFontSizeReset, panelFontSizeUp } from "./editorSettings";

export type PanelZoomStep = "up" | "down" | "reset";

/** The text size the page and the Markdown preview are laid out for before any zoom (a browser's default). */
export const PREVIEW_BASE_FONT_SIZE = 16;

/** The key's data a zoom step is read from — a `KeyboardEvent` has all of it. */
export type ZoomKeyEvent = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey">;

/**
 * Which zoom step a key press asks for, or `null` for any other key. ⌘ alone (Shift is free: `+` needs it on most
 * layouts): `+` and `=` (the same key without Shift) grow, `-` and `_` shrink, `0` resets.
 */
export function panelZoomStep(e: ZoomKeyEvent): PanelZoomStep | null {
  if (!e.metaKey || e.ctrlKey || e.altKey) return null;
  if (e.key === "+" || e.key === "=") return "up";
  if (e.key === "-" || e.key === "_") return "down";
  if (e.key === "0") return "reset";
  return null;
}

/** The window text size after `step`, from `size`. */
export function applyPanelZoomStep(size: number, step: PanelZoomStep): number {
  if (step === "up") return panelFontSizeUp(size);
  if (step === "down") return panelFontSizeDown(size);
  return panelFontSizeReset();
}

/** How much larger than their base the previews draw at window text size `size` (1 at the default). */
export function previewZoom(size: number): number {
  return size / PREVIEW_BASE_FONT_SIZE;
}
