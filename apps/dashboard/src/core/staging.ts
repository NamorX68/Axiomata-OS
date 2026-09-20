/**
 * Staged panels: a `stageable` module opened as a floating panel instead of
 * a canvas tile — how the chat and the agent hand the user a file to look
 * at. Transient: not persisted, gone on close.
 *
 * More than one can be open at once (owner request: multiple files open
 * side by side). Opening a new type/path appends a panel alongside whatever
 * is already open, each starting from the same slide-in-to-centre spot
 * (`StagingPanel.svelte`'s own `slide` transition) regardless of how many
 * others are already staged or where they've since been dragged to.
 * Opening the same type+path again — or any other interaction with an
 * already-open panel, see `bringToFront` — raises the existing one instead
 * of duplicating it. Array order is stacking order: the last entry is
 * topmost, both for `StagingPanel.svelte`'s plain document-order z-stacking
 * (every panel shares one fixed `z-index`, so later DOM order alone decides
 * ties) and for `StagingLayer.svelte`'s Escape handling, which closes the
 * last entry first.
 */

import { get, writable } from "svelte/store";

import { getModule } from "./registry";

/**
 * Where a panel should come to rest, in viewport coordinates.
 *
 * Passed as `config.anchor` by an opener that knows where the user is
 * looking. A card opened from a Kanban tile belongs over that tile, not
 * halfway across a 21:9 screen — the eye is already on the tile, and sending
 * the panel somewhere else makes the user hunt for what they just asked for.
 * Without an anchor a panel keeps the original behaviour and settles in the
 * middle of the screen, which is right for a file that has no place it
 * "came from".
 */
export interface StagingAnchor {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Reads a `config.anchor` back, tolerating anything that isn't one. */
export function readAnchor(raw: unknown): StagingAnchor | null {
  if (typeof raw !== "object" || raw === null) return null;
  const a = raw as Record<string, unknown>;
  const ok = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
  return ok(a.x) && ok(a.y) && ok(a.w) && ok(a.h) ? { x: a.x, y: a.y, w: a.w, h: a.h } : null;
}

/** The rect of the tile or panel an element sits in, for use as an anchor. */
export function hostAnchor(from: Element | null): StagingAnchor | null {
  const host = from?.closest(".tile, .panel");
  if (!host) return null;
  const rect = host.getBoundingClientRect();
  return { x: rect.left, y: rect.top, w: rect.width, h: rect.height };
}

export interface StagedPanel {
  id: string;
  type: string;
  config: Record<string, unknown>;
}

export const staged = writable<StagedPanel[]>([]);

/**
 * Opens a staged panel, or raises an already-open one for the same
 * `type` + `config.path` to the front instead of duplicating it — clicking
 * "Open" repeatedly on the same file just refocuses its existing panel,
 * since a mounted module's context only ever pushes config *out* to the
 * store (see `registry.createContext`), so there is nothing to usefully
 * overwrite. Opening a different type/path adds a new panel alongside
 * whatever's already open, rather than replacing it.
 */
export function openStaged(type: string, config: Record<string, unknown> = {}): StagedPanel | null {
  const def = getModule(type);
  if (!def?.stageable) return null;

  const existing = get(staged).find((p) => p.type === type && samePath(p.config, config));
  if (existing) {
    bringToFront(existing.id);
    return existing;
  }

  const panel: StagedPanel = { id: crypto.randomUUID(), type, config };
  staged.update((list) => [...list, panel]);
  return panel;
}

function samePath(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  return typeof a.path === "string" && a.path === b.path;
}

/** Moves a panel to the end of the stack, i.e. visually on top — the same
 *  document-order stacking trick `canvas/Tile.svelte`'s own `bringToFront`
 *  uses (there via a numeric `z`; here via array order, since every staged
 *  panel already renders at one shared fixed `z-index` and plain DOM order
 *  alone already wins ties). `StagingPanel.svelte` calls this on
 *  `pointerdowncapture`, so clicking or dragging any open panel raises it,
 *  same as clicking a tile. A no-op if `id` isn't currently staged. */
export function bringToFront(id: string): void {
  staged.update((list) => {
    const i = list.findIndex((p) => p.id === id);
    if (i === -1 || i === list.length - 1) return list;
    const panel = list[i];
    return [...list.slice(0, i), ...list.slice(i + 1), panel];
  });
}

export function closeStaged(id: string): void {
  staged.update((list) => list.filter((p) => p.id !== id));
}

export function closeAllStaged(): void {
  if (get(staged).length > 0) staged.set([]);
}
