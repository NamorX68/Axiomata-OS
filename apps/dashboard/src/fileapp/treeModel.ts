/**
 * The file tree's rules as plain functions (`docs/plans/editor.md`, ED4, W6,
 * W13): what is hidden by default, how paths are put together, whether a
 * typed name is acceptable, where an open file lands after a rename, and the
 * tree's own settings (`settings.editor.tree`: open folders, width, shown or
 * not, hidden entries shown or not).
 */

import { getSetting, setSetting } from "../core/persist";

/** Hidden unless "show hidden" is on (W6); dotfiles too. */
const HIDDEN = new Set([".git", "node_modules", "target"]);

export function isHidden(name: string): boolean {
  return HIDDEN.has(name) || name.startsWith(".");
}

/** `name` inside the folder `dir` (`""`: the root). */
export function joinRel(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name;
}

/** The folder `rel` is in (`""`: the root). */
export function parentOf(rel: string): string {
  const slash = rel.lastIndexOf("/");
  return slash < 0 ? "" : rel.slice(0, slash);
}

export function baseName(rel: string): string {
  return rel.slice(rel.lastIndexOf("/") + 1);
}

/** Why a typed file or folder name is not acceptable, or `null` if it is. */
export function nameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (!trimmed) return "A name is needed.";
  if (trimmed === "." || trimmed === "..") return "That name is taken by the file system.";
  if (/[/\u0000]/.test(trimmed)) return "A name cannot contain / or a NUL.";
  return null;
}

/**
 * Where an open file at `rel` is after `from` was renamed to `to` in the same
 * root — `from` itself, or anything under a renamed folder — or `null` if the
 * rename does not concern it.
 */
export function renamedPath(rel: string, from: string, to: string): string | null {
  if (rel === from) return to;
  return rel.startsWith(`${from}/`) ? to + rel.slice(from.length) : null;
}

/** What dropping `from` onto the folder `dir` means (ED5, T11). */
export type MoveTarget = { to: string } | { refused: string } | null;

/**
 * Moving `from` (a file or folder) into the folder `dir`: where it lands, why
 * it cannot (another root, into itself), or `null` when it is already there.
 */
export function moveTarget(from: { root: string; rel: string }, dir: { root: string; rel: string }): MoveTarget {
  if (from.root !== dir.root) return { refused: "Moving between roots is not possible — only within one." };
  // Where it is already — or dropped back on itself, a drag let go where it began.
  if (parentOf(from.rel) === dir.rel || from.rel === dir.rel) return null;
  if (isUnder(dir.rel, from.rel)) return { refused: "A folder cannot be moved into itself." };
  return { to: joinRel(dir.rel, baseName(from.rel)) };
}

/** Whether `rel` is `gone` or lies inside the folder `gone`. */
export function isUnder(rel: string, gone: string): boolean {
  return rel === gone || rel.startsWith(`${gone}/`);
}

/** The key a folder is remembered as open under. */
export function folderKey(root: string, rel: string): string {
  return `${root}\0${rel}`;
}

function splitKey(key: string): [string, string] {
  const at = key.indexOf("\0");
  return at < 0 ? [key, ""] : [key.slice(0, at), key.slice(at + 1)];
}

/** Open folders after `from` was renamed to `to` in `root`: the folder and those in it keep open under the new name. */
export function expandedAfterRename(expanded: readonly string[], root: string, from: string, to: string): string[] {
  return expanded.map((key) => {
    const [r, rel] = splitKey(key);
    const moved = r === root ? renamedPath(rel, from, to) : null;
    return moved === null ? key : folderKey(r, moved);
  });
}

/** Open folders after `rel` was deleted in `root`: it and everything in it are forgotten. */
export function expandedAfterDelete(expanded: readonly string[], root: string, rel: string): string[] {
  return expanded.filter((key) => {
    const [r, k] = splitKey(key);
    return !(r === root && isUnder(k, rel));
  });
}

/** What the sidebar column shows; the activity rail chooses (`ide/ActivityRail.svelte`). */
export type SidebarView = "files" | "search" | "git" | "agents" | "tasks";
const VIEWS: readonly SidebarView[] = ["files", "search", "git", "agents", "tasks"];

export interface TreePrefs {
  /** Which view the sidebar column shows. */
  view: SidebarView;
  /** Open folders, as `folderKey`s. */
  expanded: string[];
  /** Pixels. */
  width: number;
  visible: boolean;
  showHidden: boolean;
  /** The open project's id (`projects` table), or `null` — the tree shows that project's folder only. */
  project: number | null;
  /** The outline under the tree (#49): shown or folded away, and its height in pixels. */
  outlineOpen: boolean;
  outlineHeight: number;
}

export const DEFAULT_TREE: TreePrefs = { view: "files", expanded: [], width: 260, visible: true, showHidden: false, project: null, outlineOpen: true, outlineHeight: 240 };
/** The tree is never narrower or wider than this. */
export const TREE_WIDTH = { min: 160, max: 640 } as const;

/** The outline is never shorter or taller than this. */
export const OUTLINE_HEIGHT = { min: 80, max: 800 } as const;

export function clampOutlineHeight(height: number): number {
  return Math.round(Math.min(OUTLINE_HEIGHT.max, Math.max(OUTLINE_HEIGHT.min, height)));
}

export function clampWidth(width: number): number {
  return Math.round(Math.min(TREE_WIDTH.max, Math.max(TREE_WIDTH.min, width)));
}

/** Saved tree settings, anything malformed replaced by the default. */
export function parseTreePrefs(raw: unknown): TreePrefs {
  const r = (raw ?? {}) as Partial<Record<keyof TreePrefs, unknown>>;
  return {
    view: VIEWS.includes(r.view as SidebarView) ? (r.view as SidebarView) : DEFAULT_TREE.view,
    expanded: Array.isArray(r.expanded) ? r.expanded.filter((k): k is string => typeof k === "string") : [],
    width: typeof r.width === "number" && Number.isFinite(r.width) ? clampWidth(r.width) : DEFAULT_TREE.width,
    visible: typeof r.visible === "boolean" ? r.visible : DEFAULT_TREE.visible,
    showHidden: typeof r.showHidden === "boolean" ? r.showHidden : DEFAULT_TREE.showHidden,
    project: typeof r.project === "number" && Number.isInteger(r.project) ? r.project : null,
    outlineOpen: typeof r.outlineOpen === "boolean" ? r.outlineOpen : DEFAULT_TREE.outlineOpen,
    outlineHeight:
      typeof r.outlineHeight === "number" && Number.isFinite(r.outlineHeight)
        ? clampOutlineHeight(r.outlineHeight)
        : DEFAULT_TREE.outlineHeight,
  };
}

/** Each view of the workbench keeps its own sidebar prefs, under its own settings key. */
export type TreePrefsKey = "editor" | "ide";

export function loadTreePrefs(key: TreePrefsKey = "editor"): TreePrefs {
  return parseTreePrefs(getSetting<{ tree?: unknown }>(key)?.tree);
}

export function saveTreePrefs(prefs: TreePrefs, key: TreePrefsKey = "editor"): void {
  setSetting(key, { ...getSetting<Record<string, unknown>>(key), tree: prefs });
}
