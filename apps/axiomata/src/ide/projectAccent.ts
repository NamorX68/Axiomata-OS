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

/** Contrast against a near-white ground that a mark has to reach on a light theme (WCAG's bar for graphics is 3:1). */
const LIGHT_GROUND_CONTRAST = 4;
/** Relative luminance of the lightest ground the light themes use; the mark must be darker than the contrast allows. */
const LIGHT_GROUND_LUMINANCE = 0.94;
/** The darkest a mark is taken, so a deepened yellow turns olive rather than black. */
const MIN_LIGHTNESS = 26;

/** Relative luminance (WCAG) of an `hsl(h s% l%)` colour as {@link themeSwatches} writes it; 0 for anything else. */
function hslLuminance(h: number, s: number, l: number): number {
  const sat = s / 100;
  const light = l / 100;
  const chroma = (1 - Math.abs(2 * light - 1)) * sat;
  const channel = (n: number) => {
    const k = (n + h / 30) % 12;
    return light - (chroma / 2) * Math.max(-1, Math.min(k - 3, 9 - k, 1));
  };
  const linear = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * linear(channel(0)) + 0.7152 * linear(channel(8)) + 0.0722 * linear(channel(4));
}

/**
 * The area palette, deepened where a mark would not read on a light ground. At the same HSL lightness green, yellow and
 * cyan are far brighter to the eye than blue or violet, so on a light theme they came out at about 2.6:1 against the
 * page and a thin edge disappeared. Each hue is darkened until it reaches {@link LIGHT_GROUND_CONTRAST}; the hues that
 * already do keep their colour, and a dark theme is returned as it is.
 */
export function markerSwatches(swatches: readonly string[], light: boolean): string[] {
  if (!light) return [...swatches];
  const ceiling = (LIGHT_GROUND_LUMINANCE + 0.05) / LIGHT_GROUND_CONTRAST - 0.05;
  return swatches.map((swatch) => {
    const parsed = /^hsl\((\d+) (\d+)% (\d+)%\)$/.exec(swatch);
    if (!parsed) return swatch;
    const [h, s] = [Number(parsed[1]), Number(parsed[2])];
    let l = Number(parsed[3]);
    while (l > MIN_LIGHTNESS && hslLuminance(h, s, l) > ceiling) l -= 1;
    return `hsl(${h} ${s}% ${l}%)`;
  });
}

/** The colour of `slot` in `swatches`, or `null` for no marker (no slot, or one this palette does not have). */
export function accentColor(slot: number | null | undefined, swatches: readonly string[]): string | null {
  if (slot === null || slot === undefined || !Number.isInteger(slot) || slot < 0) return null;
  return swatches[slot] ?? null;
}

/** The theme's marker palette, read again whenever the theme (or a custom `theme.css`) changes. */
export const themeAreaSwatches: Readable<string[]> = readable<string[]>([], (set) => {
  if (typeof document === "undefined") return;
  const read = () => {
    const style = getComputedStyle(document.documentElement);
    const accent = style.getPropertyValue("--ax-accent").trim() || "#ff7a1a";
    const light = style.getPropertyValue("--ax-color-scheme").trim() === "light";
    set(markerSwatches(themeSwatches(accent, light), light));
  };
  read();
  const observer = new MutationObserver(read);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "style"] });
  observer.observe(document.head, { childList: true, subtree: true, characterData: true });
  return () => observer.disconnect();
});
