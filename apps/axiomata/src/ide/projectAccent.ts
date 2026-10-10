/**
 * The marker colour of a project (card #70): a small orientation aid, never a second accent.
 *
 * A project stores a *slot* (0–11) into the twelve hues the theme draws areas in (`themeSwatches`), not a colour: the
 * colour is looked up in the theme in effect, so it follows a theme change and reads on every ground. It reaches the
 * screen as `--ax-project-hue` on the Studio and as a few small marks (a dot, a thin edge) — `--ax-accent` is never
 * touched.
 */
import { readable, type Readable } from "svelte/store";

import { themeSwatches } from "../graph/model";

/** How many marker colours there are: the length of `themeSwatches`. */
export const ACCENT_SLOTS = 12;

/** The colour of `slot` in `swatches`, or `null` for no marker (no slot, or one this palette does not have). */
export function accentColor(slot: number | null | undefined, swatches: readonly string[]): string | null {
  if (slot === null || slot === undefined || !Number.isInteger(slot) || slot < 0) return null;
  return swatches[slot] ?? null;
}

/** The theme's area palette, read again whenever the theme (or a custom `theme.css`) changes. */
export const themeAreaSwatches: Readable<string[]> = readable<string[]>([], (set) => {
  if (typeof document === "undefined") return;
  const read = () => {
    const style = getComputedStyle(document.documentElement);
    const accent = style.getPropertyValue("--ax-accent").trim() || "#ff7a1a";
    const light = style.getPropertyValue("--ax-color-scheme").trim() === "light";
    set(themeSwatches(accent, light));
  };
  read();
  const observer = new MutationObserver(read);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "style"] });
  observer.observe(document.head, { childList: true, subtree: true, characterData: true });
  return () => observer.disconnect();
});
