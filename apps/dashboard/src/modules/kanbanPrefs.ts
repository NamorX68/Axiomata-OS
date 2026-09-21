/**
 * Kanban preferences that belong to the person rather than to one tile.
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

/**
 * How a card should sit on the column beneath it.
 *
 * `auto` leaves it to the theme, which is the right answer most of the time
 * and the default: whether a shadow or a hairline reads at all is a property
 * of the theme's brightness, not a taste (see the note on `--ax-card-*` in
 * `themes/tokens.css`). The other three exist because the owner could not
 * decide between them by description and asked to have all of them — a look
 * is judged by looking.
 */
export type CardStyle = "auto" | "flat" | "edge" | "raised";

const CARD_STYLES: CardStyle[] = ["auto", "flat", "edge", "raised"];

interface KanbanPrefs {
  /** The board a tile shows when its own instance config does not say. */
  lastBoard?: number;
  /** How cards sit on their column, across every board and every tile. */
  cardStyle?: CardStyle;
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
 * Application-wide on purpose (owner, 2026-09-21): a tile placed a moment ago
 * has no stored choice of its own, so keying this to the instance meant every
 * new tile opened whichever board happened to be first in the list — never the
 * one actually being worked on. "Last used" is a property of the person.
 *
 * The cost of that decision, stated plainly because it is a real one: with two
 * Kanban tiles open, switching one of them to peek at another board also moves
 * the starting point of every tile placed afterwards. Tiles already on the
 * canvas are unaffected — each records its own board in its instance config as
 * soon as it resolves one.
 */
export function lastBoard(): number | undefined {
  return prefs().lastBoard ?? getSetting<number>(LEGACY_LAST_BOARD_KEY);
}

/** Records the board a switcher just chose. */
export function rememberLastBoard(id: number): void {
  setSetting(KEY, { ...prefs(), lastBoard: id });
}

/**
 * The card treatment, as a store.
 *
 * A store rather than a plain read, because the two sides of a tile are two
 * components: the switch is on the flip side and the cards it changes are on
 * the front, which stays mounted the whole time — and a board opened as a
 * panel is a third instance again. All of them have to change at once, or the
 * setting looks broken until the next reload.
 */
// Read on first subscribe rather than at module load: this module is imported
// while the module registry is being built, which happens before
// `initPersistence()` has put dashboard.json into the settings bag. The first
// subscriber is a mounting Kanban component, which is comfortably after.
const style = writable<CardStyle>("auto", (set) => set(readCardStyle()));

export const cardStyle: Readable<CardStyle> = style;

export function setCardStyle(next: CardStyle): void {
  style.set(next);
  setSetting(KEY, { ...prefs(), cardStyle: next });
}

/** Tolerates a hand-edited dashboard.json naming a style that no longer
 *  exists — an unreadable preference is not worth an error, it is worth the
 *  default. */
function readCardStyle(): CardStyle {
  const stored = prefs().cardStyle;
  return stored !== undefined && CARD_STYLES.includes(stored) ? stored : "auto";
}
