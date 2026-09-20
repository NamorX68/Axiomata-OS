/**
 * One shared store per board, so every view of the same board shows the same
 * thing.
 *
 * A board can be on screen twice at once — as a tile for glancing at and as a
 * floating panel for working in — and both must agree. Each view loading its
 * own copy would work today, while the board is read-only, and would quietly
 * start lying the moment CP-K2b lets a card be dragged in one of them.
 *
 * Deliberately no polling, unlike `routines-board`: routines fire on their own
 * in the background, a board only changes when somebody changes it, and that
 * somebody is currently always this app. **That stops being true in M7.5**,
 * when agents claim cards without the user doing anything — at which point
 * this module needs a refresh path (a backend event, or a poll) rather than
 * relying on every mutation going through `refreshBoard`.
 */

import { get, writable, type Readable } from "svelte/store";

import { invokeBackend as invoke, type Board, type BoardCard, type BoardColumn } from "./backend";

export interface BoardData {
  board: Board | null;
  columns: BoardColumn[];
  cards: BoardCard[];
  /** Backend error text, or "" when the last load succeeded. */
  error: string;
  loading: boolean;
}

const EMPTY: BoardData = { board: null, columns: [], cards: [], error: "", loading: true };

const stores = new Map<number, ReturnType<typeof writable<BoardData>>>();

/**
 * The load currently running for a board, if any.
 *
 * Without this the store misses the very case it exists for: a tile and a
 * panel showing the same board mount at almost the same moment, both see
 * "not loaded yet", and both fire the same three round-trips. Harmless while
 * only this app writes — the second answer is the same as the first — but it
 * is the shared store quietly not sharing, and it gets worse the more often
 * refreshes happen.
 */
const inFlight = new Map<number, Promise<void>>();

function storeFor(boardId: number) {
  let store = stores.get(boardId);
  if (!store) {
    store = writable<BoardData>({ ...EMPTY });
    stores.set(boardId, store);
  }
  return store;
}

/** The store for a board, loading it on first use. */
export function boardStore(boardId: number): Readable<BoardData> {
  const store = storeFor(boardId);
  if (get(store).loading && get(store).board === null) void refreshBoard(boardId);
  return store;
}

/**
 * Re-reads a board from the backend.
 *
 * Archived cards are fetched too and filtered in the views, so that turning
 * the archive filter on in the panel costs nothing and cannot show a different
 * board than the tile beside it.
 */
export function refreshBoard(boardId: number): Promise<void> {
  // A load already on its way is the answer to this call too — joining it
  // beats starting a second one that would only overwrite the first with the
  // same data.
  const running = inFlight.get(boardId);
  if (running) return running;

  const task = load(boardId).finally(() => inFlight.delete(boardId));
  inFlight.set(boardId, task);
  return task;
}

async function load(boardId: number): Promise<void> {
  const store = storeFor(boardId);
  store.update((data) => ({ ...data, loading: true }));
  try {
    const [board, columns, cards] = await Promise.all([
      invoke<Board | null>("get_board", { id: boardId }),
      invoke<BoardColumn[]>("list_board_columns", { boardId }),
      invoke<BoardCard[]>("list_board_cards", { boardId, includeArchived: true }),
    ]);
    store.set({ board, columns, cards, error: "", loading: false });
  } catch (err) {
    store.update((data) => ({ ...data, error: String(err), loading: false }));
  }
}

/** Drops a board's cached store — used after the board itself is deleted. */
export function forgetBoard(boardId: number): void {
  stores.delete(boardId);
  inFlight.delete(boardId);
}

/** Test seam: clears every cached store. */
export function resetBoardStores(): void {
  stores.clear();
  inFlight.clear();
}
