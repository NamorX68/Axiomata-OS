/**
 * Kanban as an app (owner, 2026-10-04): it has no tile. The ring's "Kanban" entry opens the board as the large panel,
 * on the board that was open last.
 */

import { invokeBackend as invoke } from "../core/backend";
import { emit } from "../core/bus";
import { openStaged } from "../core/staging";
import { toast } from "../core/toast";
import { lastBoard } from "./kanbanPrefs";

/** The window the board opens in — its own size key, so a single card's panel does not inherit it. */
export const KANBAN_BOARD_SIZE_KEY = "kanban-board";
export const KANBAN_BOARD_SIZE = { w: 1920, h: 1080 };

/**
 * Which board to open: the one used last if it still exists, else the first, else none (the panel then offers to
 * create one). Pure so it can be tested without a backend.
 */
export function boardToOpen(boards: { id: number }[], remembered: number | undefined): number | null {
  if (remembered !== undefined && boards.some((board) => board.id === remembered)) return remembered;
  return boards[0]?.id ?? null;
}

/**
 * The `path` a board panel carries after it switched to `boardId` (`null` = no board). The path is what
 * `openStaged` compares, so it has to name the board the panel shows *now* — otherwise a second click on the ring
 * entry opens another panel on a board that is already open. A panel that is not a board panel (a single card's) is
 * left alone: an empty object.
 */
export function boardPathPatch(config: Record<string, unknown>, boardId: number | null): { path?: string } {
  return typeof config.path === "string" && config.path.startsWith("board:")
    ? { path: boardId === null ? "board:none" : `board:${boardId}` }
    : {};
}

/** The bus event that asks an already open board panel to show a card in its side panel. */
export const KANBAN_SHOW_CARD = "kanban:show-card";

/**
 * Opens the Kanban panel on the right board — the last one, or `target.boardId` — and, with `target.cardId`, shows that card
 * in its side panel (the spotlight's card hits). A panel opened just now reads the card from its config; one that was
 * already open is told by the bus.
 */
export async function openKanban(target: { boardId?: number; cardId?: number } = {}): Promise<void> {
  let boards: { id: number }[];
  try {
    boards = await invoke<{ id: number }[]>("list_boards");
  } catch (err) {
    // Opening on "no board" because the list failed would offer to create a board that probably exists.
    toast(`Kanban could not load the boards: ${String(err)}`, "warning");
    return;
  }
  const boardId = target.boardId ?? boardToOpen(boards, lastBoard());
  openStaged("kanban", {
    // One panel per board (a second click raises it); with no board at all, one panel to create the first in.
    path: boardId === null ? "board:none" : `board:${boardId}`,
    ...(boardId === null ? {} : { boardId }),
    ...(target.cardId === undefined ? {} : { focusCard: target.cardId }),
    sizeKey: KANBAN_BOARD_SIZE_KEY,
    panelSize: KANBAN_BOARD_SIZE,
  });
  if (target.cardId !== undefined) emit(KANBAN_SHOW_CARD, { cardId: target.cardId });
}
