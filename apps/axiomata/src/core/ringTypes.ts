/**
 * Ring entry types that were replaced (kept apart from `apps.ts` so `appGroups.ts` can use them without a cycle).
 * The Editor and IDE entries became one: Studio, whose type id is the old `view:ide`.
 */

export const RETIRED_RING_TYPES: Readonly<Record<string, string>> = {
  "view:editor": "view:ide",
  // The Kanban tile became an app that opens as a panel (owner, 2026-10-04).
  kanban: "view:kanban",
};

/**
 * Module types that no longer exist as canvas tiles. A saved tile of such a type is dropped when the dashboard
 * loads; the thing itself lives on elsewhere (Kanban as an app, with its boards in the database).
 */
export const RETIRED_TILE_TYPES: readonly string[] = ["kanban"];

/** `types` with retired ones replaced by their successor and duplicates dropped, order kept. */
export function migrateRingTypes(types: readonly string[]): string[] {
  const out: string[] = [];
  for (const type of types) {
    const next = RETIRED_RING_TYPES[type] ?? type;
    if (!out.includes(next)) out.push(next);
  }
  return out;
}
