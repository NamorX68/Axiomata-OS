/**
 * Bundled coding-monospace font catalog for the Terminal module (Checkpoint
 * 5h) — the single source of truth for which families are actually bundled,
 * mirroring how `terminalThemes.ts`'s `THEMES` is the single source for the
 * "Farbschema" setting instead of a separately hand-maintained list
 * (architecture review, Checkpoint 5h: `terminal-settings.svelte`'s
 * "Bundled font" `<select>` previously re-typed its own copy of the ten
 * family names with nothing enforcing they stayed in sync with what
 * `main.ts` actually imports — exactly the class of bug that pattern
 * already exists to prevent).
 *
 * `main.ts`'s CSS imports themselves stay separate, static `import`
 * statements — Vite needs literal, statically-analyzable import paths, not
 * something built from this array at runtime — so this module doesn't
 * eliminate that half of the duplication, only the family-*name* list a
 * consumer like the settings page needs. Adding an eleventh font means
 * updating both this array and `main.ts`'s imports; `main.ts`'s own
 * comment cross-references this file for the same reason.
 */

/** One bundled family and the static weights `main.ts` actually imports for
 *  it — not always literally 100/400/700 (`weights[0]` is each family's
 *  *lightest available* static weight, several publish no true Thin/100 at
 *  all), but always includes 400 (Regular) and 700 (Bold): every one of
 *  these eleven has a real 700 face, so "Bold" (both `terminal-settings.svelte`'s
 *  "Font weight" option and `TerminalScreen`'s separate, always-literal-
 *  `"bold"` SGR-bold rendering) is consistent across the whole set. */
export interface BundledFont {
  /** Exact `@fontsource` (or, for the one Checkpoint 5m entry, `public/fonts/`-
   *  served) CSS `font-family` name. */
  family: string;
  /** Ascending; informational (nothing currently branches on the exact
   *  values beyond documenting them in one place) — `terminal-settings.svelte`'s
   *  "Font weight" `<select>` deliberately offers the full standard 100-900
   *  scale regardless of a font's own bundled weights, see that file's own
   *  comment for why. */
  weights: readonly number[];
}

export const BUNDLED_FONTS: readonly BundledFont[] = [
  // Checkpoint 5m: the one *actually* Nerd-Fonts-patched entry in this list
  // (see `terminal-nerd-fonts.css`'s own doc comment) — every other entry
  // below relies on the separate `PureNerdFont` fallback (Checkpoint 5g)
  // for icon glyphs, which owner-reported screenshot comparisons against
  // Ghostty/Kitty showed rendering visibly different (wrong glyph shapes,
  // square instead of rounded Powerline segment caps). This is the one to
  // pick for the closest possible match to Ghostty — it's the exact same
  // font family name the owner's own Ghostty config uses.
  { family: "JetBrainsMono Nerd Font Mono", weights: [100, 400, 700] },
  { family: "JetBrains Mono", weights: [100, 400, 700] },
  { family: "IBM Plex Mono", weights: [100, 400, 700] },
  { family: "Fira Code", weights: [300, 400, 700] },
  { family: "Source Code Pro", weights: [200, 400, 700] },
  { family: "Roboto Mono", weights: [100, 400, 700] },
  { family: "Space Mono", weights: [400, 700] },
  { family: "Ubuntu Mono", weights: [400, 700] },
  { family: "Inconsolata", weights: [200, 400, 700] },
  { family: "Victor Mono", weights: [100, 400, 700] },
  { family: "Anonymous Pro", weights: [400, 700] },
];
