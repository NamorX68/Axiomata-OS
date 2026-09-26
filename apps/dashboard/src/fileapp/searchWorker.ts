/**
 * The app's one search guard (`docs/plans/editor.md`, ED5, T5): every editor
 * surface's patterns are vouched for by the same worker. Built by Vite as a
 * file of its own, so the CSP's `script-src 'self'` covers it.
 *
 * Where there is no `Worker` (the unit tests' jsdom) there is no guard, and
 * patterns run straight away as before.
 */

import { SearchGuard, type WorkerLike } from "../editor/search/guard";

let guard: SearchGuard | null | undefined;

export function searchGuard(): SearchGuard | null {
  if (guard !== undefined) return guard;
  guard =
    typeof Worker === "undefined"
      ? null
      : new SearchGuard(
          () =>
            new Worker(new URL("../editor/search/worker.ts", import.meta.url), {
              type: "module",
              name: "axiomata-search",
            }) as unknown as WorkerLike,
        );
  return guard;
}
