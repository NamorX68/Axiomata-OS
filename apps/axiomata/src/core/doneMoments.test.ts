import { describe, expect, it } from "vitest";

import type { BoardCard, BoardColumn } from "./backend";
import { arrivedDone, doneSince, rolesOf } from "./doneMoments";

const column = (id: number, maps_to_status: BoardColumn["maps_to_status"]): BoardColumn => ({
  id,
  board_id: 1,
  name: `col-${id}`,
  position: id,
  maps_to_status,
  stage: null,
});

const card = (id: number, column_id: number, extra: Partial<BoardCard> = {}): BoardCard =>
  ({
    id,
    board_id: 1,
    column_id,
    position: id,
    title: `card-${id}`,
    archived_at: null,
    updated_at: "2026-10-01T00:00:00Z",
    ...extra,
  }) as BoardCard;

const COLUMNS = [column(1, "open"), column(2, "doing"), column(3, "done")];

describe("rolesOf", () => {
  it("gives each card its column's role, and none to a card in a column the board does not list", () => {
    const roles = rolesOf([card(1, 2), card(2, 99)], COLUMNS);
    expect(roles.get(1)).toBe("doing");
    expect(roles.has(2)).toBe(false);
  });
});

describe("arrivedDone", () => {
  it("names a card that moved into a done column, from any other", () => {
    const before = rolesOf([card(1, 1), card(2, 2), card(3, 3)], COLUMNS);
    const now = rolesOf([card(1, 3), card(2, 3), card(3, 3)], COLUMNS);
    expect(arrivedDone(before, now).sort()).toEqual([1, 2]);
  });

  it("names nothing on the first look, for a card made right in done, or for one leaving done", () => {
    const now = rolesOf([card(1, 3)], COLUMNS);
    expect(arrivedDone(null, now)).toEqual([]);
    expect(arrivedDone(new Map(), now)).toEqual([]);
    expect(arrivedDone(rolesOf([card(1, 3)], COLUMNS), rolesOf([card(1, 2)], COLUMNS))).toEqual([]);
  });
});

describe("doneSince", () => {
  it("names the done cards changed after the last look, not archived ones or those elsewhere", () => {
    const cards = [
      card(1, 3, { updated_at: "2026-10-12T09:00:00Z" }),
      card(2, 3, { updated_at: "2026-10-10T09:00:00Z" }),
      card(3, 2, { updated_at: "2026-10-12T09:00:00Z" }),
      card(4, 3, { updated_at: "2026-10-12T09:00:00Z", archived_at: "2026-10-12T09:00:00Z" }),
    ];
    expect(doneSince(cards, COLUMNS, "2026-10-11T00:00:00Z")).toEqual([1]);
  });

  it("names nothing for a board never looked at, or an unreadable time", () => {
    const cards = [card(1, 3, { updated_at: "2026-10-12T09:00:00Z" })];
    expect(doneSince(cards, COLUMNS, null)).toEqual([]);
    expect(doneSince(cards, COLUMNS, "not a time")).toEqual([]);
  });
});
