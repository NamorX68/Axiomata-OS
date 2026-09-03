import { mount } from "svelte";

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
