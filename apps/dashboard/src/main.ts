import { mount } from "svelte";

import "./themes/tokens.css";
import "./themes/graphite.css";
import "./themes/paper.css";
import "./themes/steampunk.css";
import "./themes/forest.css";
import "./themes/ocean.css";
import "./styles.css";

import App from "./App.svelte";
import { DEFAULT_THEME, applyTheme } from "./core/themes";
import { registerBuiltins } from "./modules";

// Theme selection is persisted in ~/.axiomata/dashboard.json from step 6 on;
// until then, boot into the default.
applyTheme(DEFAULT_THEME);

registerBuiltins();

const target = document.getElementById("ax-shell");
if (!target) {
  throw new Error("missing #ax-shell mount point");
}

export default mount(App, { target });
