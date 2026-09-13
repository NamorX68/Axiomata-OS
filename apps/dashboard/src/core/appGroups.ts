/**
 * App-Ring group data layer: a group collapses several builtin modules, or
 * several externally added Mac apps, into one bigger ring-slot circle —
 * never both sides mixed in the same group. Same store+mutator convention
 * as `apps.ts`'s own `userApps` — `persist.ts` owns wiring `appGroups` to
 * disk (subscribes and calls `scheduleSave`). See
 * docs/plans/app-ring.md and the App-Ring-Gruppierung plan for the design.
 */

import { get, writable, type Writable } from "svelte/store";

import type { GraphNode } from "../graph/model";

/** A member is identified the same way its ring node already is: a builtin
 *  by its registry `type` (`BuiltinApp.type`), a user app by its filesystem
 *  `path` (`UserApp.path`) — no synthetic per-member id, matching those
 *  types' own identity. `side` keeps builtin and user-app groups strictly
 *  separate; nothing here ever mixes the two. */
export interface AppGroup {
  id: string;
  side: "builtin" | "user";
  /** "Gruppe N" by default (see `nextGroupName`), renamable. */
  name: string;
  /** One of `drawGlyph`'s ids (`graph/render.ts`) — "group" by default. */
  glyph: string;
  /** type[] (side "builtin") or path[] (side "user"); insertion order is
   *  the expanded secondary ring's display order. */
  members: string[];
}

export const appGroups: Writable<AppGroup[]> = writable([]);

function newId(): string {
  return crypto.randomUUID();
}

/** Next free "Gruppe N" name, gap-filling rather than counting up forever —
 *  deleting "Gruppe 2" and creating a new group should offer "Gruppe 2"
 *  again, not jump to "Gruppe 4". */
export function nextGroupName(existingNames: string[]): string {
  const used = new Set(
    existingNames
      .map((n) => /^Gruppe (\d+)$/.exec(n))
      .filter((m): m is RegExpExecArray => m !== null)
      .map((m) => Number(m[1])),
  );
  let n = 1;
  while (used.has(n)) n++;
  return `Gruppe ${n}`;
}

/** Creates a new one-member group on `side`, with the shared defaults
 *  (immediate name, generic glyph — no typing required at creation, both
 *  changeable later via `renameGroup`/`setGroupGlyph`). A group of one
 *  member is the normal starting state, not a degenerate case. */
export function createGroup(side: AppGroup["side"], memberId: string): AppGroup {
  const sameSide = get(appGroups).filter((g) => g.side === side);
  const group: AppGroup = {
    id: newId(),
    side,
    name: nextGroupName(sameSide.map((g) => g.name)),
    glyph: "group",
    members: [memberId],
  };
  appGroups.update((list) => [...list, group]);
  return group;
}

/** Adds `memberId` to `groupId`, unless it's already a member. */
export function addToGroup(groupId: string, memberId: string): void {
  appGroups.update((list) =>
    list.map((g) =>
      g.id === groupId && !g.members.includes(memberId) ? { ...g, members: [...g.members, memberId] } : g,
    ),
  );
}

/** Removes `memberId` from `groupId`. A group left with zero members
 *  dissolves entirely — its former member is simply a normal solo ring
 *  node again from the next rebuild on. */
export function removeFromGroup(groupId: string, memberId: string): void {
  appGroups.update((list) =>
    list
      .map((g) => (g.id === groupId ? { ...g, members: g.members.filter((m) => m !== memberId) } : g))
      .filter((g) => g.members.length > 0),
  );
}

/** Strips `memberId` out of every group on `side` — `apps.ts`'s
 *  `removeUserApp` calls this so a Mac app removed from the ring entirely
 *  also leaves whatever group it was in, instead of leaving a dangling
 *  member id that `buildAppNodes` would silently skip forever after. */
export function removeMemberFromAllGroups(side: AppGroup["side"], memberId: string): void {
  appGroups.update((list) =>
    list
      .map((g) => (g.side === side ? { ...g, members: g.members.filter((m) => m !== memberId) } : g))
      .filter((g) => g.members.length > 0),
  );
}

/** Blank/whitespace-only names are ignored — a group always has a name. */
export function renameGroup(id: string, name: string): void {
  const trimmed = name.trim();
  if (!trimmed) return;
  appGroups.update((list) => list.map((g) => (g.id === id ? { ...g, name: trimmed } : g)));
}

export function setGroupGlyph(id: string, glyph: string): void {
  appGroups.update((list) => list.map((g) => (g.id === id ? { ...g, glyph } : g)));
}

/** The group `memberId` (on `side`) currently belongs to, if any. */
export function groupFor(side: AppGroup["side"], memberId: string): AppGroup | undefined {
  return get(appGroups).find((g) => g.side === side && g.members.includes(memberId));
}

/** Replaces the whole list (used by `persist.ts` on boot). Does not mark
 *  anything dirty — this IS the loaded state, same convention as
 *  `apps.ts`'s `loadUserApps`. */
export function loadAppGroups(list: AppGroup[]): void {
  appGroups.set(list);
}

/** What a right-click on `node` should offer, given the current groups —
 *  the single source of truth both `AppContextMenu` and its own tests read,
 *  so "which state shows which actions" is asserted once as data instead of
 *  re-derived by hand in the component.
 *
 *  - A group's own ring-slot node (`node.isGroup`): rename / change icon —
 *    never "remove" (dissolving happens by emptying it via
 *    `removeFromGroup`, not a direct delete action) and never grouping
 *    actions (a group can't be a member of another group — no nesting).
 *  - A member node shown on that group's expanded secondary ring
 *    (`node.groupId` set, `node.isGroup` unset): only "remove from group".
 *  - An ungrouped solo node: "add to new group", plus "add to <name>" for
 *    every existing group on the *same* side (never the other side — see
 *    `AppGroup.side`); a user-app node also gets "change icon" (`core/
 *    apps.ts`'s `setUserAppGlyph` — builtins already have a sensible
 *    registry-derived glyph, so this is Mac-app-only) and "remove" —
 *    builtins are never removable from the ring at all. */
export type AppMenuAction =
  | { kind: "remove" }
  | { kind: "addToNewGroup" }
  | { kind: "addToGroup"; groupId: string; groupName: string }
  | { kind: "removeFromGroup" }
  | { kind: "rename" }
  | { kind: "changeIcon" };

export function menuActionsFor(node: GraphNode, groups: AppGroup[]): AppMenuAction[] {
  if (node.isGroup) return [{ kind: "rename" }, { kind: "changeIcon" }];
  if (node.groupId) return [{ kind: "removeFromGroup" }];
  const side: AppGroup["side"] = node.userApp ? "user" : "builtin";
  const actions: AppMenuAction[] = [{ kind: "addToNewGroup" }];
  for (const g of groups.filter((g) => g.side === side)) {
    actions.push({ kind: "addToGroup", groupId: g.id, groupName: g.name });
  }
  if (node.userApp) {
    actions.push({ kind: "changeIcon" }, { kind: "remove" });
  }
  return actions;
}
