import { mount } from "svelte";

import "./themes/tokens.css";
import "./themes/graphite.css";
import "./themes/light.css";
import "./themes/midnight.css";
import "./styles.css";

import App from "./App.svelte";
import { registerBuiltins } from "./modules";

// Theme selection is persisted in ~/.axiomata/dashboard.json from step 6 on;
// until then, default to graphite.
document.documentElement.dataset.theme = "graphite";

registerBuiltins();

const target = document.getElementById("ax-shell");
if (!target) {
  throw new Error("missing #ax-shell mount point");
}

export default mount(App, { target });
