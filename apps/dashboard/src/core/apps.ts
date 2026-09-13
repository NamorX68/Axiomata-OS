/**
 * App-Ring data layer: built-in modules from the registry (the ring's left,
 * "+"-adjacent side) and externally added Mac apps (the right side).
 *
 * `userApps` is persisted under `apps.user` in `~/.axiomata/dashboard.json`
 * by `core/persist.ts`, the same way `core/stores.ts`'s `activeTheme` is —
 * this module owns the store and its mutators, `persist.ts` owns wiring it
 * to disk (subscribes and calls `scheduleSave`). See
 * `docs/plans/app-ring.md` for the full design.
 */

import { writable, type Writable } from "svelte/store";

import { listModules } from "./registry";
import type { ModuleDefinition } from "./types";

/** A registry module shown on the ring's left (in-app) side. No `icon` field
 *  — the ring draws a hand-drawn vector glyph for builtins
 *  (`model.ts`'s `glyphForModuleType`), not the module's own
 *  `ModuleDefinition.icon` SVG, so there is nothing here to carry it in. */
export interface BuiltinApp {
  type: string;
  title: string;
}

/** Registry entries excluded from the App Ring: `background` (the Second
 *  Brain canvas itself, not a launchable tile), `dev` scaffolding (the
 *  `dummy*` modules), and `md-file` — it needs a `path` a blank ring click
 *  has no way to supply, so `createInstance("md-file")` would just produce a
 *  broken tile. Every other registered module is currently a singleton, so
 *  the ring's click handler never has to decide what a repeat click on a
 *  non-singleton builtin should do. */
function isRingEligible(def: ModuleDefinition): boolean {
  return !def.background && !def.dev && def.type !== "md-file";
}

/** Every builtin module the ring should show, in registry order — a newly
 *  registered module appears automatically, nothing to maintain here. */
export function listBuiltinApps(): BuiltinApp[] {
  return listModules()
    .filter(isRingEligible)
    .map((def) => ({ type: def.type, title: def.title }));
}

/** An externally installed Mac app the owner added via the "+" dialog.
 *  Identity is the filesystem path (`path`) — no synthetic id: this is a
 *  small, owner-curated list (via the installed-apps scan dialog), so a
 *  path collision is purely theoretical. */
export interface UserApp {
  path: string;
  name: string;
}

export const userApps: Writable<UserApp[]> = writable([]);

/** Adds `app`, unless its path is already present (no duplicate ring icons
 *  for the same app). */
export function addUserApp(app: UserApp): void {
  userApps.update((list) => (list.some((a) => a.path === app.path) ? list : [...list, app]));
}

/** Removes the app at `path`. A path that isn't present is not an error —
 *  the ring's right-click menu and the "+" dialog's toggle button both call
 *  this for the same path and may race. */
export function removeUserApp(path: string): void {
  userApps.update((list) => list.filter((a) => a.path !== path));
}

/** Replaces the whole list (used by `persist.ts` on boot). Does not mark
 *  anything dirty — this IS the loaded state, same convention as
 *  `stores.ts`'s `loadInstances`. */
export function loadUserApps(list: UserApp[]): void {
  userApps.set(list);
}
