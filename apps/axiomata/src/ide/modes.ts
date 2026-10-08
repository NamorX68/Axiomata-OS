/**
 * The workbench's three modes and their layouts (`docs/plans/workbench.md`, `docs/plans/a2a.md` A6a): **Editor** (files,
 * git, diffs), **Canvas** (the free surface of terminals and agents; stored as `agents`, its name before the Flow existed)
 * and **Flow** (planning: the plan panel and the planners' panes). A project keeps one layout per mode in its
 * `layout_json`; the modes not shown are *parked* — their panes stay mounted, hidden, so a running agent does not die
 * when the user looks at the files, or at the plan, for a while.
 *
 * Stored shape: `{ version, mode, layouts: { editor, agents, flow } }`. A row written before the modes existed holds one
 * bare layout (`{ version, root }`); that was the Agents (now Canvas) view, so it reads back as that layout. A row from
 * the two-mode time has no `flow`, which is replaced by the starting one.
 */

import type { IconName } from "../ui/icons/lucide";
import { emptyLayout, parseLayout, serializeLayout, type Layout } from "./layout";

export type Mode = "editor" | "agents" | "flow";

export const MODES: readonly Mode[] = ["editor", "agents", "flow"];

/** What the header calls each mode. The stored id of the Canvas is still `agents`. */
export const MODE_LABEL: Record<Mode, string> = { editor: "Editor", agents: "Canvas", flow: "Flow" };

/** The icon beside each mode's name in the header: code for the files, free panes for the Canvas, linked nodes for the Flow. */
export const MODE_ICON: Record<Mode, IconName> = { editor: "code", agents: "panels-top-left", flow: "workflow" };

/** The layouts of the modes that are not shown — every mode but the shown one. */
export type Parked = Partial<Record<Mode, Layout>>;

export interface Workspace {
  mode: Mode;
  /** The layout on screen. */
  active: Layout;
  /** The other modes' layouts, kept (and their panes kept running). */
  parked: Parked;
}

/**
 * Shows `next`: what was on screen is parked, what was parked for `next` is shown. A mode that has no layout yet (a
 * project from before it existed) starts empty.
 */
export function switchMode(ws: Workspace, next: Mode): Workspace {
  if (next === ws.mode) return ws;
  const { [next]: shown, ...rest } = ws.parked;
  return { mode: next, active: shown ?? emptyLayout(), parked: { ...rest, [ws.mode]: ws.active } };
}

/** The layouts by mode, whichever is shown; a mode with none yet reads as empty. */
export function layoutsOf(ws: Workspace): Record<Mode, Layout> {
  const all: Record<Mode, Layout> = { editor: emptyLayout(), agents: emptyLayout(), flow: emptyLayout() };
  for (const mode of MODES) all[mode] = mode === ws.mode ? ws.active : (ws.parked[mode] ?? all[mode]);
  return all;
}

/** Every layout that is not on screen, for the view to keep rendering (hidden) the panes of. */
export function parkedLayouts(parked: Parked): Layout[] {
  return MODES.flatMap((mode) => (parked[mode] ? [parked[mode]] : []));
}

export function serializeWorkspace(ws: Workspace): string {
  const layouts = layoutsOf(ws);
  return JSON.stringify({
    version: 1,
    mode: ws.mode,
    layouts: {
      editor: JSON.parse(serializeLayout(layouts.editor)),
      agents: JSON.parse(serializeLayout(layouts.agents)),
      flow: JSON.parse(serializeLayout(layouts.flow)),
    },
  });
}

function parkedOf(all: Record<Mode, Layout>, shown: Mode): Parked {
  const parked: Parked = {};
  for (const mode of MODES) if (mode !== shown) parked[mode] = all[mode];
  return parked;
}

/**
 * Reads a stored `layout_json`. `null` when nothing usable is in it (the caller starts fresh and
 * says so); a missing half is replaced by `fallback`'s, so one damaged layout does not lose the others.
 */
export function parseWorkspace(raw: unknown, fallback: Record<Mode, () => Layout>): Workspace | null {
  let value = raw;
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch {
      return null;
    }
  }
  if (typeof value !== "object" || value === null) return null;
  const record = value as Record<string, unknown>;

  const layouts = record.layouts;
  if (typeof layouts === "object" && layouts !== null) {
    const l = layouts as Record<string, unknown>;
    const editor = parseLayout(l.editor);
    const agents = parseLayout(l.agents);
    const flow = parseLayout(l.flow);
    // Nothing usable at all: not even the two older modes' halves.
    if (!editor && !agents && !flow) return null;
    const mode: Mode = MODES.includes(record.mode as Mode) ? (record.mode as Mode) : "agents";
    const all: Record<Mode, Layout> = {
      editor: editor ?? fallback.editor(),
      agents: agents ?? fallback.agents(),
      flow: flow ?? fallback.flow(),
    };
    return { mode, active: all[mode], parked: parkedOf(all, mode) };
  }

  const legacy = parseLayout(record);
  if (!legacy) return null;
  const all: Record<Mode, Layout> = { editor: fallback.editor(), agents: legacy, flow: fallback.flow() };
  return { mode: "agents", active: legacy, parked: parkedOf(all, "agents") };
}

/** The Editor layout of a project that has none yet: nothing open. */
export function emptyEditorLayout(): Layout {
  return emptyLayout();
}
