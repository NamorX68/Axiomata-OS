/**
 * The vault's notes and tags as the editor needs them for wikilinks (`docs/plans/tags-links.md`, TL2): one copy for
 * every editor, read from `get_workspace_graph` (the scan the Second Brain draws) and the tag list (`get_tag_list`).
 *
 * * **Read on demand, kept a while**: the scan walks the whole vault, so it runs when a Markdown note of the workspace
 *   first asks, and again only after a file came or went (or was renamed) or after {@link MAX_AGE_MS} — never per
 *   keystroke.
 * * **The vault is also where a project is opened on it**: a root whose folder is the workspace's (or lies inside it)
 *   holds vault notes too ({@link vaultPrefix}), so the Studio's tree and tabs on the vault folder link as the
 *   workspace does.
 * * **Asked synchronously for drawing**: {@link peekNoteIndex} answers at once with what is loaded (or `null`), and
 *   whoever draws subscribes ({@link onNoteIndex}) to draw again once a fresher copy is in.
 */

import {
  invokeBackend,
  listenBackend,
  type FileChange,
  type FileRootInfo,
  type TagEntry,
  type WorkspaceGraph,
} from "../core/backend";
import { listRoots } from "./backend";
import { NoteIndex } from "./noteLinks";

/** How long a read of the vault is trusted when no file event said otherwise. */
export const MAX_AGE_MS = 60_000;

let index: NoteIndex | null = null;
let readAt = 0;
let loading: Promise<NoteIndex | null> | null = null;
let stale = true;
let tagList: Promise<TagEntry[]> | null = null;
const listeners = new Set<() => void>();
let watching = false;

/** Starts listening for files that come and go in the workspace, once. */
function watch(): void {
  if (watching) return;
  watching = true;
  // Any root may be the vault (a project opened on its folder); marking it stale costs nothing until the next ask.
  const forget = (_root: string) => invalidateNoteIndex();
  void listenBackend<FileChange>("files:changed", (c) => {
    // Only a note that came or went changes the links; a saved note's new tags wait for the age limit, since
    // autosave would otherwise rescan the vault every second while typing.
    if (c.kind !== "modified") forget(c.root);
  }).catch(() => undefined);
  void listenBackend<{ root: string }>("files:renamed", (c) => forget(c.root)).catch(() => undefined);
  void listenBackend<{ root: string }>("files:removed", (c) => forget(c.root)).catch(() => undefined);
}

/** The next read of the vault starts afresh; what is loaded stays usable until then. */
export function invalidateNoteIndex(): void {
  stale = true;
  tagList = null;
}

/** What is loaded now, for drawing; starts a fresh read in the background when it is old. */
export function peekNoteIndex(): NoteIndex | null {
  if (stale || Date.now() - readAt > MAX_AGE_MS) void loadNoteIndex();
  return index;
}

/** The vault's notes, read when what is loaded is old; `null` when the scan fails (no workspace). */
export function loadNoteIndex(): Promise<NoteIndex | null> {
  watch();
  if (!stale && index && Date.now() - readAt <= MAX_AGE_MS) return Promise.resolve(index);
  if (loading) return loading;
  stale = false;
  loading = invokeBackend<WorkspaceGraph>("get_workspace_graph")
    .then((graph) => {
      index = new NoteIndex(graph.files);
      readAt = Date.now();
      for (const listener of [...listeners]) listener();
      return index;
    })
    .catch(() => {
      stale = true;
      return index;
    })
    .finally(() => {
      loading = null;
    });
  return loading;
}

/** Calls `listener` after every fresh read; returns how to stop. */
export function onNoteIndex(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The vault's tag list (`TAGS.md`), read once per change of the vault; empty when there is none yet. */
export function loadTagList(): Promise<TagEntry[]> {
  watch();
  tagList ??= invokeBackend<TagEntry[]>("get_tag_list").catch(() => {
    tagList = null;
    return [];
  });
  return tagList;
}

let roots: Promise<FileRootInfo[]> | null = null;

/**
 * Where `rootPath` lies in the vault at `workspacePath`: `""` for the vault's own folder, the folder's path inside it
 * for one deeper, `null` outside it.
 */
export function prefixIn(workspacePath: string, rootPath: string): string | null {
  const w = workspacePath.replace(/\/+$/, "");
  const r = rootPath.replace(/\/+$/, "");
  if (!w || !r) return null;
  if (r === w) return "";
  return r.startsWith(`${w}/`) ? r.slice(w.length + 1) : null;
}

/** Where the root `root` lies in the vault (see {@link prefixIn}); `"workspace"` is the vault itself. */
export async function vaultPrefix(root: string): Promise<string | null> {
  if (root === "workspace") return "";
  roots ??= listRoots().catch(() => {
    roots = null;
    return [];
  });
  const all = await roots;
  const workspace = all.find((r) => r.kind === "workspace");
  const here = all.find((r) => r.id === root);
  // A worktree or a granted file is a copy or a single file — never where the vault's notes live.
  if (!workspace || !here || (here.kind !== "project" && here.kind !== "grant-folder")) return null;
  return prefixIn(workspace.path, here.path);
}

/** A path in the vault as seen from a root lying at `prefix` in it: the same file under that root's own path. */
export function vaultRel(prefix: string, rel: string): string {
  return prefix ? `${prefix}/${rel}` : rel;
}

/** A vault path as a file of the root at `prefix`, or `null` when it lies outside that root. */
export function rootRel(prefix: string, vaultPath: string): string | null {
  if (!prefix) return vaultPath;
  return vaultPath.startsWith(`${prefix}/`) ? vaultPath.slice(prefix.length + 1) : null;
}

/** Forgets everything — for tests. */
export function resetNoteIndexForTests(): void {
  roots = null;
  index = null;
  readAt = 0;
  loading = null;
  stale = true;
  tagList = null;
  listeners.clear();
}
