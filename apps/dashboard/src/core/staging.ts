/**
 * Staged panels: a `stageable` module opened as a slide-in panel (from the
 * bottom or the right) instead of a canvas tile — how the chat and the agent
 * hand the user a file to look at. Transient: not persisted, gone on close.
 */

import { get, writable } from "svelte/store";

import { getModule } from "./registry";

export type StageFrom = "bottom" | "right";

export interface StagedPanel {
  id: string;
  type: string;
  config: Record<string, unknown>;
  from: StageFrom;
}

export const staged = writable<StagedPanel[]>([]);

/**
 * Opens a staged panel. Every side (`right` / `bottom`) holds at most one —
 * they are all rendered at the same fixed position, so more than one per
 * side would silently stack on top of each other. Opening the same `type` +
 * `config.path` again (e.g. clicking "Open" repeatedly) is a no-op: the
 * existing panel is left exactly as it is, since a mounted module's context
 * only ever pushes config *out* to the store (see `registry.createContext`),
 * so there is nothing to usefully overwrite. Opening something else on an
 * occupied side replaces it with a freshly mounted panel.
 */
export function openStaged(
  type: string,
  config: Record<string, unknown> = {},
  from: StageFrom = "right",
): StagedPanel | null {
  const def = getModule(type);
  if (!def?.stageable) return null;

  const existing = get(staged).find((p) => p.from === from);
  if (existing && existing.type === type && samePath(existing.config, config)) {
    return existing;
  }

  const panel: StagedPanel = { id: crypto.randomUUID(), type, config, from };
  staged.update((list) => [...list.filter((p) => p.from !== from), panel]);
  return panel;
}

function samePath(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  return typeof a.path === "string" && a.path === b.path;
}

export function closeStaged(id: string): void {
  staged.update((list) => list.filter((p) => p.id !== id));
}

export function closeAllStaged(): void {
  if (get(staged).length > 0) staged.set([]);
}
