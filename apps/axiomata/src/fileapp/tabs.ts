/**
 * The file app's tab records and the reader for tabs saved before groups
 * existed (`docs/plans/editor.md`, ED4, W7, W12). The tabs themselves — one per
 * file, the preview tab, closing, the keys, keeping them — now live in groups
 * (retired with the old editor view, `docs/plans/workbench.md` step 4); `settings.editor.tabs` is read once, by
 * `loadDock`, and replaced by `settings.editor.dock`.
 */

export interface FileRef {
  root: string;
  rel: string;
}

export interface Tab {
  id: string;
  /** `null`: a new note, not filed yet. */
  file: FileRef | null;
  /** The reusable look-only tab (W7); at most one per group. */
  preview: boolean;
}

export interface TabsState {
  tabs: Tab[];
  active: string | null;
}

export const NO_TABS: TabsState = { tabs: [], active: null };

function sameFile(a: FileRef | null, b: FileRef | null): boolean {
  if (a === null || b === null) return a === b;
  return a.root === b.root && a.rel === b.rel;
}

/** How tabs were saved before groups existed. */
interface SavedTabs {
  tabs: Array<{ root: string; rel: string; preview: boolean } | { newNote: true }>;
  active: number;
}

/** Tabs saved before groups existed (`settings.editor.tabs`) back into a state; anything malformed is skipped. */
export function parseTabs(raw: unknown, newId: () => string): TabsState {
  const saved = raw as Partial<SavedTabs> | null;
  if (!saved || !Array.isArray(saved.tabs)) return NO_TABS;
  const tabs: Tab[] = [];
  for (const entry of saved.tabs) {
    const e = entry as Record<string, unknown> | null;
    if (!e || typeof e !== "object") continue;
    if (e.newNote === true) {
      if (!tabs.some((t) => t.file === null)) tabs.push({ id: newId(), file: null, preview: false });
    } else if (typeof e.root === "string" && typeof e.rel === "string" && e.rel) {
      const file = { root: e.root, rel: e.rel };
      if (!tabs.some((t) => sameFile(t.file, file))) tabs.push({ id: newId(), file, preview: e.preview === true });
    }
  }
  const index = typeof saved.active === "number" ? saved.active : 0;
  return { tabs, active: (tabs[index] ?? tabs[0])?.id ?? null };
}
