/**
 * Named 16-colour ANSI palette data for the terminal's "Farbschema/Theme"
 * setting (Checkpoint 5b of docs/plans/terminal.md). Pulled out of
 * `TerminalScreen.ts` (architecture review) since it's plain colour-table
 * data, not rendering logic — `TerminalScreen.ts`'s own module doc comment
 * scopes that file to "pure Canvas-2D rendering," and this data is consumed
 * by non-rendering callers too (`terminal-settings.svelte`'s theme
 * `<select>`, `terminal.svelte`'s `config.theme` resolution), not just
 * `TerminalScreen.draw`.
 */

/** Standard xterm 16-colour palette (SGR 30-37/90-97 map to indices 0-15) —
 *  the base ANSI colours most CLI tools assume, and this module's own
 *  original (only) palette before Checkpoint 5b. This is `THEMES.xterm`. */
const ANSI_16: readonly string[] = [
  "#000000",
  "#cd0000",
  "#00cd00",
  "#cdcd00",
  "#0000ee",
  "#cd00cd",
  "#00cdcd",
  "#e5e5e5",
  "#7f7f7f",
  "#ff0000",
  "#00ff00",
  "#ffff00",
  "#5c5cff",
  "#ff00ff",
  "#00ffff",
  "#ffffff",
];

/**
 * Named 16-colour ANSI palettes for Checkpoint 5b's "Farbschema/Theme"
 * setting — `xterm` (the palette this module always drew before this
 * checkpoint) plus six publicly documented, widely-used community
 * palettes (four from Checkpoint 5b, plus Catppuccin Mocha and Tokyo Night
 * added in Checkpoint 5g). Only indices 0-15 are themeable; the 256-colour cube
 * (16-231) and greyscale ramp (232-255) are a fixed algorithm, not part
 * of any named palette, same as real terminals (`TerminalScreen.indexedToCss`
 * only consults a palette for index < 16).
 *
 * Sources (all standard, commonly replicated 16-colour ANSI tables, no
 * licensing concerns): Solarized Dark is Ethan Schoonover's official
 * `base03`/accent mapping (ethanschoonover.com/solarized); Dracula is the
 * project's own published "ANSI Colors" (draculatheme.com/contribute);
 * Nord and Gruvbox Dark are each project's own widely-adopted terminal
 * palette (nordtheme.com; github.com/morhetz/gruvbox); Catppuccin Mocha is
 * the project's own published terminal ANSI mapping, reused verbatim by its
 * official terminal ports (catppuccin.com; github.com/catppuccin/catppuccin
 * — "Mocha" flavour); Tokyo Night is the `tokyonight.nvim`/VS Code theme's
 * own widely-replicated terminal mapping (its `terminal.integrated.colors`,
 * base "Night" variant, not Storm/Light) — both added in Checkpoint 5g,
 * owner request for "mehr Themes (Catppuccin, Tokio etc.)".
 *
 * Adding a theme here also means adding its own entry to
 * `THEME_DEFAULT_COLORS` below (skip only for a theme that's deliberately
 * meant to inherit the app's own chrome-theme colours, `xterm`'s own
 * choice) — `terminalThemes.test.ts` enforces this, but it's easy to forget
 * since the two tables are otherwise edited independently.
 */
export const THEMES: Record<string, readonly string[]> = {
  xterm: ANSI_16,
  "solarized-dark": [
    "#073642",
    "#dc322f",
    "#859900",
    "#b58900",
    "#268bd2",
    "#d33682",
    "#2aa198",
    "#eee8d5",
    "#002b36",
    "#cb4b16",
    "#586e75",
    "#657b83",
    "#839496",
    "#6c71c4",
    "#93a1a1",
    "#fdf6e3",
  ],
  dracula: [
    "#21222c",
    "#ff5555",
    "#50fa7b",
    "#f1fa8c",
    "#bd93f9",
    "#ff79c6",
    "#8be9fd",
    "#f8f8f2",
    "#6272a4",
    "#ff6e6e",
    "#69ff94",
    "#ffffa5",
    "#d6acff",
    "#ff92df",
    "#a4ffff",
    "#ffffff",
  ],
  nord: [
    "#3b4252",
    "#bf616a",
    "#a3be8c",
    "#ebcb8b",
    "#81a1c1",
    "#b48ead",
    "#88c0d0",
    "#e5e9f0",
    "#4c566a",
    "#bf616a",
    "#a3be8c",
    "#ebcb8b",
    "#81a1c1",
    "#b48ead",
    "#8fbcbb",
    "#eceff4",
  ],
  "gruvbox-dark": [
    "#282828",
    "#cc241d",
    "#98971a",
    "#d79921",
    "#458588",
    "#b16286",
    "#689d6a",
    "#a89984",
    "#928374",
    "#fb4934",
    "#b8bb26",
    "#fabd2f",
    "#83a598",
    "#d3869b",
    "#8ec07c",
    "#ebdbb2",
  ],
  "catppuccin-mocha": [
    "#45475a",
    "#f38ba8",
    "#a6e3a1",
    "#f9e2af",
    "#89b4fa",
    "#f5c2e7",
    "#94e2d5",
    "#bac2de",
    "#585b70",
    "#f38ba8",
    "#a6e3a1",
    "#f9e2af",
    "#89b4fa",
    "#f5c2e7",
    "#94e2d5",
    "#a6adc8",
  ],
  "tokyo-night": [
    "#15161e",
    "#f7768e",
    "#9ece6a",
    "#e0af68",
    "#7aa2f7",
    "#bb9af7",
    "#7dcfff",
    "#a9b1d6",
    "#414868",
    "#f7768e",
    "#9ece6a",
    "#e0af68",
    "#7aa2f7",
    "#bb9af7",
    "#7dcfff",
    "#c0caf5",
  ],
};

/** `THEMES`' key for its default/fallback palette — used whenever
 *  `TerminalScreen.resolveColor`/`draw` isn't given an explicit `palette`,
 *  and whenever `config.theme` is unset or names a palette that doesn't
 *  exist (`terminal.svelte`'s `currentPalette`, `terminal-settings.svelte`'s
 *  `<select>`). Exported (unlike its Checkpoint-5b-original, file-private
 *  form) so those callers reference one source of truth instead of each
 *  re-typing the literal `"xterm"`. */
export const DEFAULT_THEME = "xterm";

/** Each named palette's own authentic default background/foreground —
 *  the colour an *unwritten* cell (`TermColor` `"default"`) actually shows,
 *  as opposed to the 16 indexed ANSI colours in `THEMES` above, which only
 *  ever apply to explicitly-coloured text/backgrounds a program requests.
 *  Checkpoint 5k, owner feedback: "die Hintergrundfarbe... passt zwar zum
 *  App Theme aber nicht zu[m] Theme des Terminals" — `terminal.svelte`
 *  previously always sourced its default bg/fg from the *app's own* chrome
 *  theme (`--ax-surface-1`/`--ax-text`), completely independent of
 *  `terminalSettings.theme` — so switching to "Catppuccin Mocha" recoloured
 *  every 16-colour-indexed thing (prompt segments, `ls` colours, …) but the
 *  actual background stayed whatever the app's own theme (graphite/paper/…)
 *  happened to be, visibly clashing with the rest of the theme.
 *
 *  `xterm` is deliberately absent here, not an oversight: it's this app's
 *  own "inherit the app's chrome theme" default (unchanged pre-5k
 *  behaviour), not a real named palette with its own established identity
 *  the way the other six are — there's no single "authentic xterm
 *  background" to be faithful to the way there is for Catppuccin's own
 *  published Base/Text colours. `terminal.svelte`'s resolution order:
 *  this table's entry for the active theme, else the CSS-derived app-theme
 *  colour, exactly as before this checkpoint. Sources: same per-theme
 *  projects `THEMES`' own doc comment already cites for the 16-colour
 *  tables (their standard/official "background"+"foreground" values, not
 *  invented). */
export const THEME_DEFAULT_COLORS: Partial<Record<string, { background: string; foreground: string }>> = {
  "solarized-dark": { background: "#002b36", foreground: "#839496" },
  dracula: { background: "#282a36", foreground: "#f8f8f2" },
  nord: { background: "#2e3440", foreground: "#d8dee9" },
  "gruvbox-dark": { background: "#282828", foreground: "#ebdbb2" },
  "catppuccin-mocha": { background: "#1e1e2e", foreground: "#cdd6f4" },
  "tokyo-night": { background: "#1a1b26", foreground: "#c0caf5" },
};
