import { describe, expect, it } from "vitest";

import type { BoardCard, BoardColumn } from "./backend";
import {
  actorLabel,
  applyFilter,
  collectLabels,
  dueState,
  groupByColumn,
  labelColorIndex,
  showsAssignee,
  statusOf,
} from "./kanban";

function column(id: number, position: number, maps_to_status: BoardColumn["maps_to_status"]): BoardColumn {
  return { id, board_id: 1, name: `col-${id}`, position, maps_to_status };
}

function card(id: number, column_id: number, position: number, extra: Partial<BoardCard> = {}): BoardCard {
  return {
    id,
    board_id: 1,
    column_id,
    position,
    title: `card-${id}`,
    body: "",
    labels: [],
    assignee: null,
    claimed_by: null,
    claimed_at: null,
    verified_by: null,
    verified_at: null,
    due_at: null,
    archived_at: null,
    created_at: "2026-09-01T00:00:00Z",
    updated_at: "2026-09-01T00:00:00Z",
    ...extra,
  };
}

const COLUMNS = [column(1, 1, "open"), column(2, 2, "doing"), column(3, 3, "done")];

describe("groupByColumn", () => {
  it("orders columns and the cards inside them by position", () => {
    const grouped = groupByColumn(
      [column(3, 3, "done"), column(1, 1, "open")],
      [card(10, 1, 2), card(11, 1, 1), card(12, 3, 1)],
    );

    expect(grouped.map((g) => g.column.id)).toEqual([1, 3]);
    expect(grouped[0].cards.map((c) => c.id)).toEqual([11, 10]);
    expect(grouped[1].cards.map((c) => c.id)).toEqual([12]);
  });

  it("breaks a position tie by id so the order is still stable", () => {
    const grouped = groupByColumn([column(1, 1, "open")], [card(20, 1, 5), card(19, 1, 5)]);
    expect(grouped[0].cards.map((c) => c.id)).toEqual([19, 20]);
  });

  it("drops a card whose column does not exist instead of inventing a home for it", () => {
    const grouped = groupByColumn([column(1, 1, "open")], [card(30, 99, 1)]);
    expect(grouped[0].cards).toEqual([]);
  });

  it("keeps an empty column rather than omitting it", () => {
    const grouped = groupByColumn(COLUMNS, []);
    expect(grouped).toHaveLength(3);
    expect(grouped.every((g) => g.cards.length === 0)).toBe(true);
  });
});

describe("statusOf", () => {
  it("reads the status off the column, never off the card", () => {
    expect(statusOf(card(1, 3, 1), COLUMNS)).toBe("done");
    expect(statusOf(card(1, 2, 1), COLUMNS)).toBe("doing");
  });

  it("returns null for a card pointing at no known column", () => {
    expect(statusOf(card(1, 99, 1), COLUMNS)).toBeNull();
  });
});

describe("applyFilter", () => {
  const cards = [
    card(1, 1, 1, { title: "Migration prüfen", labels: ["rust"] }),
    card(2, 1, 2, { title: "Vault-Spiegel", body: "Markdown nach Kanban/", labels: ["rust", "vault"] }),
    card(3, 1, 3, { title: "Archiviertes", archived_at: "2026-09-01T00:00:00Z" }),
    card(4, 1, 4, { title: "Fällig", due_at: "2026-09-10T00:00:00Z" }),
  ];

  it("hides archived cards unless asked for them", () => {
    expect(applyFilter(cards).map((c) => c.id)).toEqual([1, 2, 4]);
    expect(applyFilter(cards, { includeArchived: true }).map((c) => c.id)).toEqual([1, 2, 3, 4]);
  });

  it("matches text against title and body, ignoring case", () => {
    expect(applyFilter(cards, { text: "migration" }).map((c) => c.id)).toEqual([1]);
    expect(applyFilter(cards, { text: "markdown" }).map((c) => c.id)).toEqual([2]);
  });

  it("requires every requested label, not just one of them", () => {
    expect(applyFilter(cards, { labels: ["rust"] }).map((c) => c.id)).toEqual([1, 2]);
    expect(applyFilter(cards, { labels: ["rust", "vault"] }).map((c) => c.id)).toEqual([2]);
  });

  it("drops undated cards when filtering by due date", () => {
    const due = applyFilter(cards, { dueBefore: new Date("2026-09-30T00:00:00Z") });
    expect(due.map((c) => c.id)).toEqual([4]);
  });

  it("returns everything when given no filter at all", () => {
    expect(applyFilter(cards, {})).toHaveLength(3);
  });
});

describe("collectLabels", () => {
  it("deduplicates and sorts", () => {
    const labels = collectLabels([
      card(1, 1, 1, { labels: ["vault", "rust"] }),
      card(2, 1, 2, { labels: ["rust", "design"] }),
    ]);
    expect(labels).toEqual(["design", "rust", "vault"]);
  });
});

describe("dueState", () => {
  // Built in local time on purpose. The badge answers "is this today for the
  // person looking at it", which is a calendar question in their timezone —
  // a UTC fixture would make this test pass or fail depending on where it runs.
  const local = (day: number, hour: number) => new Date(2026, 8, day, hour).toISOString();
  const now = new Date(2026, 8, 20, 12);

  it("calls something due late today 'heute', not 'in 0 T'", () => {
    expect(dueState(local(20, 23), now)).toEqual({ label: "heute", overdue: false });
  });

  it("counts calendar days, so tomorrow early is still tomorrow", () => {
    expect(dueState(local(21, 6), now)).toEqual({ label: "morgen", overdue: false });
  });

  it("marks a past date overdue with how far past it is", () => {
    expect(dueState(local(18, 12), now)).toEqual({
      label: "2 T überfällig",
      overdue: true,
    });
  });

  it("counts further-out dates in days", () => {
    expect(dueState(local(25, 12), now).label).toBe("in 5 T");
  });
});

describe("labelColorIndex", () => {
  it("gives the same label the same colour every time", () => {
    expect(labelColorIndex("rust", 6)).toBe(labelColorIndex("rust", 6));
  });

  it("stays inside the palette", () => {
    for (const label of ["rust", "vault", "design", "a11y", "", "sehr langes Label"]) {
      const index = labelColorIndex(label, 6);
      expect(index).toBeGreaterThanOrEqual(0);
      expect(index).toBeLessThan(6);
    }
  });
});

describe("actorLabel", () => {
  it("drops the kind prefix", () => {
    expect(actorLabel("agent:claude-1")).toBe("claude-1");
    expect(actorLabel("human:owner")).toBe("owner");
  });

  it("leaves a string without a prefix alone", () => {
    expect(actorLabel("owner")).toBe("owner");
  });
});

describe("showsAssignee", () => {
  it("stays quiet about the one human and about nobody", () => {
    expect(showsAssignee(null)).toBe(false);
    expect(showsAssignee("human:owner")).toBe(false);
  });

  it("names everyone else, which is what lights up once agents arrive", () => {
    expect(showsAssignee("agent:claude-1")).toBe(true);
    expect(showsAssignee("human:someone-else")).toBe(true);
  });
});
