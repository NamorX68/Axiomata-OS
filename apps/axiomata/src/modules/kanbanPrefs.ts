/**
 * Kanban preferences that belong to the person rather than to one panel.
 *
 * Kept in one namespaced object under `settings.kanban` in dashboard.json,
 * the way `SecondBrainView` keeps its own under `settings.secondBrain` — not
 * as bare top-level keys. The settings bag is flat and shared by the whole
 * shell, so every module that files a preference under its own name instead
 * of its own namespace is one more chance for two modules to pick the same
 * one, and there is nothing at compile time to catch it.
 *
 * It lives beside the module rather than in `core/` because it is the
 * module's own state; `core/kanban.ts` stays free of persistence so it
 * remains testable as pure logic.
 */

import { writable, type Readable } from "svelte/store";

import { getSetting, setSetting } from "../core/persist";

interface KanbanPrefs {
  /** The board a tile shows when its own instance config does not say. */
  lastBoard?: number;
  /**
   * A card's colour stripe from its first label (editor-look B6), across every
   * board and every tile. There used to be a choice of four card treatments
   * here (`cardStyle`); one look remains (B7), and a stored choice is ignored.
   */
  cardStripes?: boolean;
}

const KEY = "kanban";

/** The flat key this preference lived under before it was namespaced. Read
 *  once as a fallback so an install that already remembers a board keeps it;
 *  nothing writes it any more. */
const LEGACY_LAST_BOARD_KEY = "kanbanLastBoard";

function prefs(): KanbanPrefs {
  return getSetting<KanbanPrefs>(KEY) ?? {};
}

/**
 * The board most recently chosen in any Kanban switcher.
 *
 * Application-wide on purpose (owner, 2026-09-21): a panel opened a moment ago
 * has no stored choice of its own, so keying this to the panel meant every
 * new panel opened whichever board happened to be first in the list — never the
 * one actually being worked on. "Last used" is a property of the person.
 *
 * The cost of that decision, stated plainly because it is a real one: with two
 * Kanban panels open, switching one of them to peek at another board also moves
 * the starting point of every panel opened afterwards. Panels already open are
 * unaffected — each records its own board in its config as soon as it resolves
 * one.
 */
export function lastBoard(): number | undefined {
  return prefs().lastBoard ?? getSetting<number>(LEGACY_LAST_BOARD_KEY);
}

/** Records the board a switcher just chose. */
export function rememberLastBoard(id: number): void {
  setSetting(KEY, { ...prefs(), lastBoard: id });
}

/**
 * Whether cards carry their colour stripe, as a store.
 *
 * A store rather than a plain read, because the switch (in the boards popover) and the cards it changes (in every
 * open board panel) are different components, and they have to change at once, or the setting looks broken until the
 * next reload.
 */
// Read on first subscribe rather than at module load: this module is imported
// while the module registry is being built, which happens before
// `initPersistence()` has put dashboard.json into the settings bag. The first
// subscriber is a mounting Kanban component, which is comfortably after.
const stripes = writable<boolean>(true, (set) => set(prefs().cardStripes !== false));

export const cardStripes: Readable<boolean> = stripes;

export function setCardStripes(next: boolean): void {
  stripes.set(next);
  setSetting(KEY, { ...prefs(), cardStripes: next });
}
