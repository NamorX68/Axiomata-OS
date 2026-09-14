import { mount } from "svelte";

// `tokens.css` must load before the per-theme files: its neutral `:root`
// fallback and each theme's `[data-theme="…"]` block share the same
// specificity (0,1,0) on purpose (see `themes/tokens.css`), so a tie between
// them resolves by CSS source order, not selector strength. Reordering these
// imports would make the fallback palette win over every theme.
import "./themes/tokens.css";
import "./themes/graphite.css";
import "./themes/paper.css";
import "./themes/steampunk.css";
import "./themes/forest.css";
import "./themes/ocean.css";
import "./styles.css";
import "./core/markdown-syntax.css";
// Icon glyphs (App Ring, Second-Brain legend, area/group icons) — static
// weight-400 file only, deliberately not the variable-font package: canvas
// `ctx.font` can't set `font-variation-settings`, so a variable font would
// render at undefined default axis values. See `graph/render.ts`'s
// `drawGlyph`.
import "@fontsource/material-symbols-rounded/400.css";
// Terminal module (Checkpoint 5g, owner request: "einige MonoFonts... in
// Thin, Normal und Bold") — two bundled coding-monospace families, in the
// three weights the owner asked for (100/400/700 → Thin/Regular/Bold), not
// every static weight `@fontsource` ships. Only Regular and Bold are
// actually reachable today: `TerminalScreen.draw` only ever prepends
// `"bold "` or nothing per cell (see its own doc comment), and the Terminal
// settings picker (`terminal-settings.svelte`'s "Bundled font" `<select>`)
// only chooses a font *family*, not a weight. The 100 (Thin) face is
// bundled anyway per the literal request and for parity between the two
// families — nothing in the app currently renders text at that weight.
// Picked specifically because both, unlike e.g. Fira Code, publish a real
// 100 (Thin) static weight.
import "@fontsource/jetbrains-mono/100.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/700.css";
import "@fontsource/ibm-plex-mono/100.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/700.css";
// Icon-glyphs-only Nerd Font build (MIT, `azurity/pure-nerd-font` — not the
// official nerd-fonts.com project, a small community repackaging of its
// glyph set as a standalone ~1MB webfont meant to be layered onto any other
// font via a CSS/canvas font-family fallback list, rather than one of the
// official project's own fully patched ~10-40MB monospace families).
// `terminal.svelte`'s `currentFont()` appends `"PureNerdFont"` as a fallback
// after whatever family is actually configured, so Powerline/glyph icons in
// a Starship/Powerlevel10k prompt render regardless of font choice — this
// is what the owner's own Ghostty-comparison screenshots (Checkpoint 5e)
// showed missing. Caveat (architecture review, Checkpoint 5g): the bundled
// `.woff2` is pinned to Nerd Fonts v2.2.0-RC (its own CSS's comment) and
// the npm package hasn't published since 2025-08-24 — a preset that relies
// on an icon codepoint added in a newer upstream Nerd Fonts release may
// still show a blank box even after this fix. Pinned to an exact version
// (`package.json`, no `^`) rather than the caret range the other two font
// packages use, specifically because it's a single-maintainer binary-glyph
// repack with no test coverage of its own content — an unreviewed minor
// bump could silently change glyph coverage.
import "@azurity/pure-nerd-font/pure-nerd-font.css";

import App from "./App.svelte";
import { startAgentBridge } from "./core/agent-bridge";
import { loadCustomTheme } from "./core/custom-theme";
import { initPersistence } from "./core/persist";
import { DEFAULT_THEME, applyTheme } from "./core/themes";
import { registerBuiltins } from "./modules";

// Paint the default theme immediately; `initPersistence` swaps in the saved
// one (and the saved layout) as soon as ~/.axiomata/dashboard.json is read.
applyTheme(DEFAULT_THEME);

registerBuiltins();

const target = document.getElementById("ax-shell");
if (!target) {
  throw new Error("missing #ax-shell mount point");
}

const app = mount(App, { target });
void initPersistence().then(() => {
  void loadCustomTheme();
  startAgentBridge();
});

export default app;
