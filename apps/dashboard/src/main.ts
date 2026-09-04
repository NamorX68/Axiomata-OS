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
