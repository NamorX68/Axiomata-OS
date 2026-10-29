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
 * looking. A card opened from a tile belongs over that tile, not
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

/**
 * How big a panel should open, stated by the opener.
 *
 * Passed as `config.panelSize`, alongside `config.sizeKey`. One module can
 * serve windows of very different shapes — a Kanban board and a single Kanban
 * card are the same module — so a remembered size filed under the module type
 * alone made the card inherit the board's. `sizeKey` is what the remembered
 * size is filed under (the module type when the opener says nothing), and
 * `panelSize` is where a shape starts before the user has resized it.
 *
 * Together with `anchor` and `path`, this is the whole of what an opener may
 * tell the panel shell about how to present itself. Everything else in
 * `config` belongs to the module and the shell does not look at it.
 */
export interface PanelSize {
  w: number;
  h: number;
}

/** Reads a `config.panelSize` back, tolerating anything that isn't one. */
export function readPanelSize(raw: unknown): PanelSize | null {
  if (typeof raw !== "object" || raw === null) return null;
  const s = raw as Record<string, unknown>;
  const ok = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v) && v > 0;
  return ok(s.w) && ok(s.h) ? { w: s.w, h: s.h } : null;
}

/**
 * A remembered size out of a setting that files sizes by key (`stagingPanelSizes` per module type, `windowSizes` per
 * `Window.svelte` `sizeKey`); `null` when the setting or its entry is not one.
 */
export function rememberedSize(sizes: unknown, key: string): PanelSize | null {
  if (typeof sizes !== "object" || sizes === null || Array.isArray(sizes)) return null;
  return readPanelSize((sizes as Record<string, unknown>)[key]);
}

/** The same setting with `size` filed under `key`, every other entry kept — what is written back after a resize. */
export function rememberingSize(sizes: unknown, key: string, size: PanelSize): Record<string, unknown> {
  const kept = typeof sizes === "object" && sizes !== null && !Array.isArray(sizes) ? sizes : {};
  return { ...(kept as Record<string, unknown>), [key]: size };
}

/** What a panel's remembered size is filed under: the opener's `config.sizeKey`
 *  if it stated one, else the module type. */
export function readSizeKey(config: Record<string, unknown>, type: string): string {
  return typeof config.sizeKey === "string" && config.sizeKey !== "" ? config.sizeKey : type;
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

/** The file panel's module type (`fileapp/FilePanel.svelte`, editor plan ED4). */
export const FILE_PANEL = "file";

/**
 * Opens a workspace file in the file panel — `"read"` on its rendered view
 * for Markdown and HTML, `"edit"` on the source (editor plan W1, W10).
 */
export function openFilePanel(path: string, mode: "read" | "edit" = "read"): StagedPanel | null {
  return openStaged(FILE_PANEL, { path, mode });
}

/** A new note in the file panel (W4); only one at a time. */
export function openNewNote(): StagedPanel | null {
  return openStaged(FILE_PANEL, { path: "", mode: "edit", isNew: true });
}

/** The same file: the same `path` under the same `root` (both unset for the workspace). */
function samePath(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  return typeof a.path === "string" && a.path === b.path && a.root === b.root;
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
  closeGuards.delete(id);
  lossProbes.delete(id);
  staged.update((list) => list.filter((p) => p.id !== id));
}

/**
 * Asked before a panel closes by the owner's hand (×, Escape, ⌘W): resolves
 * `true` to let it go. The file panel asks about unsaved text this way
 * (`docs/plans/editor.md`, ED4, W11). `closeStaged` itself never asks.
 */
export type CloseGuard = () => Promise<boolean>;

const closeGuards = new Map<string, CloseGuard>();

/** Registers (or with `null` removes) the guard of panel `id`. */
export function setCloseGuard(id: string, guard: CloseGuard | null): void {
  if (guard) closeGuards.set(id, guard);
  else closeGuards.delete(id);
}

/**
 * What panel `id` would lose if it closed now, said for the owner ("unsaved changes to Notes/a.md"), or `null` for
 * nothing. Asked *before* a close nobody's hand started — the chat's `close_view` (`docs/plans/chat-knows-app.md`,
 * CH5) closes by itself only when this is `null`, and asks the owner otherwise. Synchronous on purpose: it reads
 * state, it never asks anything itself — that stays the guard's.
 */
export type LossProbe = () => string | null;

const lossProbes = new Map<string, LossProbe>();

/** Registers (or with `null` removes) what panel `id` would lose by closing — beside its guard, not instead of it. */
export function setLossProbe(id: string, probe: LossProbe | null): void {
  if (probe) lossProbes.set(id, probe);
  else lossProbes.delete(id);
}

/** What panel `id` would lose by closing now; `null` for nothing (or a panel that registered no probe). */
export function pendingLoss(id: string): string | null {
  return lossProbes.get(id)?.() ?? null;
}

/** Closes panel `id` once its guard, if any, agrees. */
export async function requestClose(id: string): Promise<void> {
  const guard = closeGuards.get(id);
  if (guard && !(await guard())) return;
  closeStaged(id);
}

