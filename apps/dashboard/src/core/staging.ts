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

export function openStaged(
  type: string,
  config: Record<string, unknown> = {},
  from: StageFrom = "right",
): StagedPanel | null {
  const def = getModule(type);
  if (!def?.stageable) return null;
  const panel: StagedPanel = { id: crypto.randomUUID(), type, config, from };
  staged.update((list) => [...list, panel]);
  return panel;
}

export function closeStaged(id: string): void {
  staged.update((list) => list.filter((p) => p.id !== id));
}

export function closeAllStaged(): void {
  if (get(staged).length > 0) staged.set([]);
}
