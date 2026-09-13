/**
 * Staged panels: a `stageable` module opened as a slide-in panel from the
 * bottom of the screen instead of a canvas tile — how the chat and the
 * agent hand the user a file to look at. Transient: not persisted, gone on
 * close.
 */

import { get, writable } from "svelte/store";

import { getModule } from "./registry";

export interface StagedPanel {
  id: string;
  type: string;
  config: Record<string, unknown>;
}

export const staged = writable<StagedPanel[]>([]);

/**
 * Opens a staged panel. At most one is ever open — every panel renders at
 * the same fixed position, so a second one would silently stack on top of
 * the first. Opening the same `type` + `config.path` again (e.g. clicking
 * "Open" repeatedly) is a no-op: the existing panel is left exactly as it
 * is, since a mounted module's context only ever pushes config *out* to the
 * store (see `registry.createContext`), so there is nothing to usefully
 * overwrite. Opening anything else replaces whatever is currently staged
 * with a freshly mounted panel.
 */
export function openStaged(type: string, config: Record<string, unknown> = {}): StagedPanel | null {
  const def = getModule(type);
  if (!def?.stageable) return null;

  const existing = get(staged)[0];
  if (existing && existing.type === type && samePath(existing.config, config)) {
    return existing;
  }

  const panel: StagedPanel = { id: crypto.randomUUID(), type, config };
  staged.set([panel]);
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
