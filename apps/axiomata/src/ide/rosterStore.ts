/**
 * The engine catalog as the Agents panel sees it. Engines are made in one place only, the settings
 * (`EnginesSection`); the panel merely *chooses* from them. Both sides read this store, and whoever changes the
 * catalog calls [`refreshEngines`], so the panel's picker never shows a stale list.
 */

import { writable } from "svelte/store";

import { messageOf } from "../core/errors";
import { listEngines, type EngineEntry } from "../core/roster";
import { toast } from "../core/toast";

const store = writable<EngineEntry[]>([]);

/** The catalog, with how many sessions run on each engine. */
export const engineCatalog = { subscribe: store.subscribe };

/** Reads the catalog again. A failure is shown once and leaves the last list in place. */
export async function refreshEngines(): Promise<EngineEntry[]> {
  try {
    const engines = await listEngines();
    store.set(engines);
    return engines;
  } catch (err) {
    toast(`Could not load the engines: ${messageOf(err)}`, "warning");
    return [];
  }
}

/** What an engine is called in a picker: its label, and what it runs when the label does not say. */
export function engineLine(engine: Pick<EngineEntry, "harness" | "model">): string {
  const harness = engine.harness === "claude_code" ? "Claude Code" : engine.harness === "mini" ? "Mini" : "Opencode";
  return `${harness} · ${engine.model ?? "default model"}`;
}
