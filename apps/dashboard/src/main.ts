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
// Terminal module — ten bundled coding-monospace families (Checkpoint 5g
// started with two; Checkpoint 5h added eight more plus a real "Font weight"
// setting, owner request: "auch gerne noch ein paar Monoschriften auch wenn
// sie kein Thin etc. anbieten... so 10 Fonts wären toll"). Each imports its
// lightest available static weight (not always literally 100/Thin — several
// of these families don't publish one, see per-font notes below), 400
// (Regular), and 700 (Bold) — every one of the ten actually has a real 700
// face, so "Bold" (both the settings picker's own "Bold" weight option and
// `TerminalScreen`'s separate SGR-bold rendering, which always uses the
// literal CSS `"bold"` keyword regardless of the chosen regular weight) is
// consistent across the whole set. `terminal-settings.svelte`'s "Font weight"
// `<select>` (Checkpoint 5h) lets the owner actually choose a *regular-text*
// weight from the full standard 100-900 scale — not limited to just these
// three per font; a weight this file didn't import a face for still renders,
// via the browser's own standard nearest-available-weight matching against
// whatever *is* registered here (the normal way missing static weights are
// handled for any web font, not a bug). `./modules/terminalFonts.ts`'s
// `BUNDLED_FONTS` is the declared, single-source-of-truth family list this
// block's imports must stay in sync with by hand (architecture review,
// Checkpoint 5h) — Vite needs these as literal, statically-analyzable
// `import` paths, so that catalog can't drive them directly; update both
// when adding/removing a font.
import "@fontsource/jetbrains-mono/100.css"; // has a real Thin (100)
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/700.css";
import "@fontsource/ibm-plex-mono/100.css"; // has a real Thin (100)
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/700.css";
import "@fontsource/fira-code/300.css"; // lightest available is Light (300), no Thin/ExtraLight
import "@fontsource/fira-code/400.css";
import "@fontsource/fira-code/700.css";
import "@fontsource/source-code-pro/200.css"; // lightest available is ExtraLight (200)
import "@fontsource/source-code-pro/400.css";
import "@fontsource/source-code-pro/700.css";
import "@fontsource/roboto-mono/100.css"; // has a real Thin (100)
import "@fontsource/roboto-mono/400.css";
import "@fontsource/roboto-mono/700.css";
import "@fontsource/space-mono/400.css"; // only ships Regular/Bold — owner's explicit "even without Thin" case
import "@fontsource/space-mono/700.css";
import "@fontsource/ubuntu-mono/400.css"; // only ships Regular/Bold — owner's explicit "even without Thin" case
import "@fontsource/ubuntu-mono/700.css";
import "@fontsource/inconsolata/200.css"; // lightest available is ExtraLight (200)
import "@fontsource/inconsolata/400.css";
import "@fontsource/inconsolata/700.css";
import "@fontsource/victor-mono/100.css"; // has a real Thin (100)
import "@fontsource/victor-mono/400.css";
import "@fontsource/victor-mono/700.css";
import "@fontsource/anonymous-pro/400.css"; // only ships Regular/Bold — owner's explicit "even without Thin" case
import "@fontsource/anonymous-pro/700.css";
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
// The real, officially Nerd-Fonts-patched "JetBrainsMono Nerd Font Mono" —
// see this file's own doc comment for the full story (Checkpoint 5m: a
// single cohesive patched font, not a fallback chain, to genuinely match
// what Ghostty/Kitty render).
import "./modules/terminal-nerd-fonts.css";

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
