/**
 * App-Ring data layer: built-in modules from the registry (the ring's left,
 * "+"-adjacent side) and externally added Mac apps (the right side).
 *
 * `userApps` and `hiddenBuiltins` are persisted under `apps.user`/
 * `apps.hiddenBuiltins` in `~/.axiomata/dashboard.json` by `core/persist.ts`,
 * the same way `core/stores.ts`'s `activeTheme` is — this module owns the
 * stores and their mutators, `persist.ts` owns wiring them to disk
 * (subscribes and calls `scheduleSave`). See `docs/plans/app-ring.md` for
 * the full design (`hiddenBuiltins`: Checkpoint 5c).
 */

import { get, writable, type Writable } from "svelte/store";

import { removeMemberFromAllGroups } from "./appGroups";
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

/** Registry entries excluded from the App Ring, unconditionally (the owner
 *  has no way to bring these back — see `hiddenBuiltins` below for the
 *  ones they *can* toggle): `background` (the Second Brain canvas itself,
 *  not a launchable tile), `dev` scaffolding (the `dummy*` modules), and
 *  `md-file` — it needs a `path` a blank ring click has no way to supply,
 *  so `createInstance("md-file")` would just produce a broken tile.
 *
 *  `terminal` (or any other `singleton: false` builtin) is otherwise
 *  ring-eligible like everything else — `second-brain.svelte`'s
 *  `handleAppClick` checks `ModuleDefinition.singleton` at click time and
 *  always creates a fresh instance for a non-singleton, rather than the
 *  bring-existing-to-front behaviour every singleton builtin gets, so there
 *  is no "which one wins" ambiguity to sidestep by excluding it here (as an
 *  earlier version of this function did — `docs/plans/terminal.md`
 *  Checkpoint 1's original exclusion, superseded by the App Ring's
 *  Checkpoint 5c "Tools und Apps verwaltbar machen"). */
function isRingEligible(def: ModuleDefinition): boolean {
  return !def.background && !def.dev && def.type !== "md-file";
}

/** Every ring-*eligible* builtin, regardless of whether the owner has hidden
 *  it (`hiddenBuiltins` below) — what the "+" dialog's "Intern" tab lists,
 *  so a hidden one can be found again and shown. Registry order, same as
 *  `listBuiltinApps`. */
export function listAllRingEligibleBuiltins(): BuiltinApp[] {
  return listModules()
    .filter(isRingEligible)
    .map((def) => ({ type: def.type, title: def.title }));
}

/** Every builtin module the ring should actually *draw* — ring-eligible
 *  and not hidden. A newly registered module appears automatically (until
 *  the owner hides it), nothing to maintain here. */
export function listBuiltinApps(): BuiltinApp[] {
  const hidden = new Set(get(hiddenBuiltins));
  return listAllRingEligibleBuiltins().filter((a) => !hidden.has(a.type));
}

/** Registry `type`s of builtin Tools/Apps the owner has hidden from the
 *  ring via the "+" dialog's "Intern" tab (Checkpoint 5c of
 *  `docs/plans/app-ring.md`) — every ring-eligible builtin shows
 *  automatically until explicitly hidden here, the opposite default of
 *  `userApps` (which starts empty and is opted into one app at a time). A
 *  plain `string[]`, not a `Set`, so it round-trips through
 *  `JSON.stringify` in `persist.ts`'s `buildState` unchanged, same
 *  convention as every other persisted store in this module. */
export const hiddenBuiltins: Writable<string[]> = writable([]);

/** Hides `type` from the ring, unless it's already hidden. No existence
 *  check against the registry — a type that later becomes ring-ineligible
 *  or gets removed just never shows up in `listAllRingEligibleBuiltins`
 *  either, so a stale hidden entry is inert, not a dangling reference to
 *  clean up. */
export function hideBuiltinApp(type: string): void {
  hiddenBuiltins.update((list) => (list.includes(type) ? list : [...list, type]));
}

/** Un-hides `type`. A type that wasn't hidden is not an error — the "+"
 *  dialog's toggle button calls this unconditionally based on its own
 *  displayed state, same convention as `removeUserApp`. */
export function showBuiltinApp(type: string): void {
  hiddenBuiltins.update((list) => list.filter((t) => t !== type));
}

/** Replaces the whole list (used by `persist.ts` on boot). Does not mark
 *  anything dirty — this IS the loaded state, same convention as
 *  `loadUserApps`. */
export function loadHiddenBuiltins(list: string[]): void {
  hiddenBuiltins.set(list);
}

/** An externally installed Mac app the owner added via the "+" dialog.
 *  Identity is the filesystem path (`path`) — no synthetic id: this is a
 *  small, owner-curated list (via the installed-apps scan dialog), so a
 *  path collision is purely theoretical. */
export interface UserApp {
  path: string;
  name: string;
  /** One of `drawGlyph`'s ids (`graph/render.ts`), chosen via the ring's
   *  "Symbol ändern" action (same `GlyphPicker` a group uses). `undefined`
   *  until the owner picks one — the ring then falls back to a monogram of
   *  `name`, since there's no way to extract a Mac app's real icon (v1). */
  glyph?: string;
}

export const userApps: Writable<UserApp[]> = writable([]);

/** Adds `app`, unless its path is already present (no duplicate ring icons
 *  for the same app). */
export function addUserApp(app: UserApp): void {
  userApps.update((list) => (list.some((a) => a.path === app.path) ? list : [...list, app]));
}

/** Removes the app at `path`. A path that isn't present is not an error —
 *  the ring's right-click menu and the "+" dialog's toggle button both call
 *  this for the same path and may race. Also drops it from whatever App-
 *  Ring group it was a member of (see `appGroups.ts`) — done here, once,
 *  rather than at every call site, so a future third caller can't forget
 *  it: a Mac app removed from the ring entirely can't stay a dangling
 *  group member. */
export function removeUserApp(path: string): void {
  userApps.update((list) => list.filter((a) => a.path !== path));
  removeMemberFromAllGroups("user", path);
}

/** Sets (or clears, with `undefined`) the ring icon override for the app at
 *  `path`. A path that isn't present is a no-op, same convention as
 *  `removeUserApp`. */
export function setUserAppGlyph(path: string, glyph: string | undefined): void {
  userApps.update((list) => list.map((a) => (a.path === path ? { ...a, glyph } : a)));
}

/** Replaces the whole list (used by `persist.ts` on boot). Does not mark
 *  anything dirty — this IS the loaded state, same convention as
 *  `stores.ts`'s `loadInstances`. */
export function loadUserApps(list: UserApp[]): void {
  userApps.set(list);
}
