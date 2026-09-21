/**
 * Gives a pane the `ModuleContext` a module would otherwise get from `Tile.svelte`.
 *
 * The whole point of the IDE view is that it hosts the modules that already
 * exist rather than reimplementing them: a terminal in a pane is the same
 * `modules/terminal.svelte` the canvas mounts, with a different thing supplying
 * its context. Everything a module may rely on is kept — `invoke`, `emit` and a
 * writable `config` that persists — so nothing needs an "am I in a tile or a
 * pane" branch.
 *
 * Two deliberate differences from the canvas, both from where a pane's state
 * lives:
 *
 * * A pane's config is stored **in the layout tree** (`PaneTab.config`, written
 *   to a project's `layout_json`), not in `dashboard.json`. A pane is not a
 *   canvas instance and has no row there.
 * * `requestResize` does nothing. On the canvas a tile owns its size; in a dock
 *   the tree does, and a module that asked to be 400px wide would be asking the
 *   wrong authority. Modules treat it as a request, not a command — the
 *   terminal, the first pane content, never calls it and measures itself with a
 *   `ResizeObserver` instead.
 */

import { createContext } from "../core/registry";
import type { ModuleContext } from "../core/types";

import type { PaneTab } from "./layout";

/**
 * Builds the context for one pane.
 *
 * `onConfig` receives every change after the initial value — the caller writes
 * it back into the tab and saves the layout. `tab.id` becomes the module's
 * `instanceId`, so a module keying anything off it (a Tauri session, a store
 * entry) gets one identity per pane that lasts as long as the tab does.
 */
export function paneContext(tab: PaneTab, onConfig: (config: Record<string, unknown>) => void): ModuleContext {
  return createContext(tab.id, tab.config ?? {}, onConfig);
}
