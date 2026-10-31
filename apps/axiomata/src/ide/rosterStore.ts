/**
 * The engine catalog as the Agents panel sees it. Engines are made in one place only, the settings
 * (`EnginesSection`); the panel merely *chooses* from them. Both sides read this store, and whoever changes the
 * catalog calls [`refreshEngines`], so the panel's picker never shows a stale list.
 */

import { writable } from "svelte/store";

import { messageOf } from "../core/errors";
import { listEngines, type EngineEntry } from "../core/roster";
import { toast } from "../core/toast";
import { tAgents } from "../i18n/agents";

const store = writable<EngineEntry[]>([]);

const rosterVersionStore = writable(0);

/**
 * Counts changes of the roles. The Agents panel reads the roles a project has once per project; the roles are made in
 * another pane, so whoever saves or deletes one calls [`bumpRoster`] and the panel's "Start" chips follow.
 */
export const rosterVersion = { subscribe: rosterVersionStore.subscribe };

export function bumpRoster(): void {
  rosterVersionStore.update((n) => n + 1);
}

/** The catalog, with how many sessions run on each engine. */
export const engineCatalog = { subscribe: store.subscribe };

/** Reads the catalog again. A failure is shown once and leaves the last list in place. */
export async function refreshEngines(): Promise<EngineEntry[]> {
  try {
    const engines = await listEngines();
    store.set(engines);
    return engines;
  } catch (err) {
    toast(tAgents("enginesUnreadable", { error: messageOf(err) }), "warning");
    return [];
  }
}

/** What an engine is called in a picker: its label, and what it runs when the label does not say. */
export function engineLine(engine: Pick<EngineEntry, "harness" | "model">): string {
  const harness = engine.harness === "claude_code" ? "Claude Code" : engine.harness === "glut" ? "Glut" : "Opencode";
  return `${harness} · ${engine.model ?? tAgents("defaultModel")}`;
}
