/**
 * Ring entry types that were replaced (kept apart from `apps.ts` so `appGroups.ts` can use them without a cycle).
 * The Editor and IDE entries became one: Studio, whose type id is the old `view:ide`.
 */

export const RETIRED_RING_TYPES: Readonly<Record<string, string>> = { "view:editor": "view:ide" };

/** `types` with retired ones replaced by their successor and duplicates dropped, order kept. */
export function migrateRingTypes(types: readonly string[]): string[] {
  const out: string[] = [];
  for (const type of types) {
    const next = RETIRED_RING_TYPES[type] ?? type;
    if (!out.includes(next)) out.push(next);
  }
  return out;
}
