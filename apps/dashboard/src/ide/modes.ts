/**
 * The workbench's two modes and their layouts (`docs/plans/workbench.md`): **Editor** (files, git,
 * diffs) and **Agents** (the full layout with terminals and agents). A project keeps one layout per
 * mode in its `layout_json`; the mode not shown is *parked* — its panes stay mounted, hidden, so
 * a running agent does not die when the user looks at the files for a while.
 *
 * Stored shape: `{ version, mode, layouts: { editor, agents } }`. A row written before the modes
 * existed holds one bare layout (`{ version, root }`); that was the agents' view, so it reads back
 * as the Agents layout, with an empty Editor layout beside it.
 */

import { emptyLayout, parseLayout, serializeLayout, type Layout } from "./layout";

export type Mode = "editor" | "agents";

export const MODES: readonly Mode[] = ["editor", "agents"];

export interface Workspace {
  mode: Mode;
  /** The layout on screen. */
  active: Layout;
  /** The other mode's layout, kept (and its panes kept running). */
  parked: Layout;
}

export function otherMode(mode: Mode): Mode {
  return mode === "editor" ? "agents" : "editor";
}

/** Swaps the shown mode: what was on screen is parked, what was parked is shown. */
export function switchMode(ws: Workspace): Workspace {
  return { mode: otherMode(ws.mode), active: ws.parked, parked: ws.active };
}

/** The layouts by mode, whichever is shown. */
export function layoutsOf(ws: Workspace): Record<Mode, Layout> {
  return ws.mode === "editor" ? { editor: ws.active, agents: ws.parked } : { editor: ws.parked, agents: ws.active };
}

export function serializeWorkspace(ws: Workspace): string {
  const layouts = layoutsOf(ws);
  return JSON.stringify({
    version: 1,
    mode: ws.mode,
    layouts: {
      editor: JSON.parse(serializeLayout(layouts.editor)),
      agents: JSON.parse(serializeLayout(layouts.agents)),
    },
  });
}

/**
 * Reads a stored `layout_json`. `null` when nothing usable is in it (the caller starts fresh and
 * says so); a missing half is replaced by `fallback`'s, so one damaged layout does not lose the other.
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
    if (!editor && !agents) return null;
    const mode: Mode = record.mode === "editor" ? "editor" : "agents";
    const both = { editor: editor ?? fallback.editor(), agents: agents ?? fallback.agents() };
    return { mode, active: both[mode], parked: both[otherMode(mode)] };
  }

  const legacy = parseLayout(record);
  return legacy ? { mode: "agents", active: legacy, parked: fallback.editor() } : null;
}

/** The Editor layout of a project that has none yet: nothing open. */
export function emptyEditorLayout(): Layout {
  return emptyLayout();
}
