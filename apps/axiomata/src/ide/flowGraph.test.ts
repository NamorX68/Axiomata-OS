import { describe, expect, it } from "vitest";

import type { BoardCard, TaskState } from "../core/backend";
import {
  END,
  NODE_H,
  NODE_W,
  START,
  START_H,
  clip,
  endOf,
  hasReviewStage,
  layersOf,
  layoutGraph,
  reviewLabel,
  reviewToneOf,
  toneOf,
} from "./flowGraph";

function card(id: number, depends_on: number[] = [], state: TaskState = "ready"): BoardCard {
  return { id, depends_on, state, title: `c${id}`, returned_count: 0 } as BoardCard;
}

describe("the columns of a plan", () => {
  it("puts a card one column right of the deepest card it needs", () => {
    // 3 needs 1 and 2; 2 needs 1: 1 | 2 | 3.
    const layers = layersOf([card(1), card(2, [1]), card(3, [1, 2]), card(4)]);
    expect([...layers.entries()]).toEqual([[1, 0], [2, 1], [3, 2], [4, 0]]);
  });

  it("ignores a card that is not in the plan and does not hang on a cycle that got in", () => {
    expect(layersOf([card(1, [99])]).get(1)).toBe(0);
    const cyclic = layersOf([card(1, [2]), card(2, [1])]);
    expect(cyclic.size).toBe(2);
  });
});

describe("the layout", () => {
  it("is empty for no cards", () => {
    expect(layoutGraph([])).toEqual({ start: null, end: null, nodes: [], edges: [], width: 0, height: 0 });
  });

  it("gives each card a place, a line for each edge, and room for all of them", () => {
    const graph = layoutGraph([card(1), card(2, [1]), card(3, [1]), card(4, [2, 3])]);
    expect(graph.nodes).toHaveLength(4);
    expect(graph.edges.map((e) => [e.from, e.to])).toEqual([[START, 1], [1, 2], [1, 3], [2, 4], [3, 4], [4, END]]);
    const nodes = new Map(graph.nodes.map((n) => [n.card.id, n]));
    expect(nodes.get(2)!.x).toBeGreaterThan(nodes.get(1)!.x + NODE_W - 1);
    // 2 and 3 share a column: different rows, no overlap.
    expect(nodes.get(2)!.x).toBe(nodes.get(3)!.x);
    expect(Math.abs(nodes.get(2)!.y - nodes.get(3)!.y)).toBeGreaterThanOrEqual(NODE_H);
    for (const node of graph.nodes) {
      expect(node.x + NODE_W).toBeLessThanOrEqual(graph.width);
      expect(node.y + NODE_H).toBeLessThanOrEqual(graph.height);
    }
  });

  it("orders a column by the rows of what its cards need, so two chains do not cross", () => {
    // Two chains a1→a2, b1→b2, with the ids interleaved so the id order alone would cross them.
    const graph = layoutGraph([card(1), card(2), card(3, [2]), card(4, [1])]);
    const row = (id: number) => graph.nodes.find((n) => n.card.id === id)!.y;
    expect(row(4) < row(3)).toBe(row(1) < row(2));
  });

  it("draws a line from the right side of the card needed to the left side of the card that needs it", () => {
    const graph = layoutGraph([card(1), card(2, [1])]);
    const edge = graph.edges.find((e) => e.from === 1)!;
    const [from, to] = [graph.nodes[0], graph.nodes[1]];
    expect(edge.path.startsWith(`M ${from.x + NODE_W} ${from.y + NODE_H / 2}`)).toBe(true);
    expect(edge.path.endsWith(`${to.x} ${to.y + NODE_H / 2}`)).toBe(true);
  });
});

describe("the start and the middle line", () => {
  it("lets every card that needs none grow out of one start node left of the first column", () => {
    const graph = layoutGraph([card(1), card(2), card(3, [1, 2])]);
    const starts = graph.edges.filter((e) => e.from === START).map((e) => e.to);
    expect(starts).toEqual([1, 2]);
    const first = Math.min(...graph.nodes.map((n) => n.x));
    expect(graph.start!.x + graph.start!.w).toBeLessThanOrEqual(first);
  });

  it("treats a card whose predecessors are all outside the plan as one that needs none", () => {
    const graph = layoutGraph([card(1, [99])]);
    expect(graph.edges.map((e) => [e.from, e.to])).toEqual([[START, 1], [1, END]]);
  });

  it("hangs every column around the middle of the tallest one, and the start on the same line", () => {
    // Column 0 has three cards, column 1 has one: the single card sits level with the middle of the three.
    const graph = layoutGraph([card(1), card(2), card(3), card(4, [1, 2, 3])]);
    const centre = (y: number, h: number) => y + h / 2;
    const middle = graph.nodes.find((n) => n.card.id === 2)!;
    const alone = graph.nodes.find((n) => n.card.id === 4)!;
    expect(centre(alone.y, NODE_H)).toBeCloseTo(centre(middle.y, NODE_H));
    expect(centre(graph.start!.y, START_H)).toBeCloseTo(centre(middle.y, NODE_H));
    for (const node of graph.nodes) expect(node.y + NODE_H).toBeLessThanOrEqual(graph.height);
  });
});

describe("how it is drawn", () => {
  it("groups the states into the families the stylesheet colours", () => {
    expect(toneOf("working")).toBe("active");
    expect(toneOf("input_required")).toBe("attention");
    expect(toneOf("in_review")).toBe("review");
    for (const done of ["verified", "integrated", "done", "taken_over"] as TaskState[]) expect(toneOf(done)).toBe("done");
    expect(toneOf("failed")).toBe("failed");
    expect(toneOf("ready")).toBe("idle");
    expect(toneOf("proposed")).toBe("idle");
  });

  it("cuts a title to its first line and a length that fits", () => {
    expect(clip("short")).toBe("short");
    expect(clip("first\nsecond")).toBe("first");
    expect(clip("x".repeat(50), 10)).toBe(`${"x".repeat(9)}…`);
  });
});

describe("the review after a card", () => {
  it("shows once a card is handed in, signed off or was sent back, and not before", () => {
    expect(hasReviewStage({ state: "working", returned_count: 0 })).toBe(false);
    expect(hasReviewStage({ state: "ready", returned_count: 0 })).toBe(false);
    expect(hasReviewStage({ state: "in_review", returned_count: 0 })).toBe(true);
    expect(hasReviewStage({ state: "integrated", returned_count: 0 })).toBe(true);
    expect(hasReviewStage({ state: "working", returned_count: 1 })).toBe(true);
  });

  it("is a node of its own between the card and the cards that wait for it", () => {
    // 1 is in review, 2 needs 1: 1 | review of 1 | 2.
    const graph = layoutGraph([card(1, [], "in_review"), card(2, [1], "blocked")]);
    expect(graph.nodes.map((n) => [n.key, n.kind, n.layer])).toEqual([
      [1, "card", 0],
      [-1, "review", 1],
      [2, "card", 2],
    ]);
    expect(graph.edges.map((e) => [e.from, e.to])).toEqual([[START, 1], [1, -1], [-1, 2], [2, END]]);
  });

  it("hangs a card that needs a reviewed and an unreviewed card on the review of the one and on the other itself", () => {
    // 1 is signed off, 2 is still working, 3 needs both: 3 waits for the review of 1 and for 2.
    const graph = layoutGraph([card(1, [], "verified"), card(2, [], "working"), card(3, [1, 2], "blocked")]);
    expect(graph.edges.filter((e) => e.to === 3).map((e) => e.from).sort()).toEqual([-1, 2]);
    // 3 sits behind the review of 1, two columns right of it, and so right of 2 as well.
    const column = (key: number) => graph.nodes.find((n) => n.key === key)!.layer;
    expect(column(3)).toBe(column(-1) + 1);
    expect(column(3)).toBeGreaterThan(column(2));
  });

  it("leaves a card without a review stage as it was drawn before", () => {
    const graph = layoutGraph([card(1), card(2, [1])]);
    expect(graph.nodes.map((n) => n.kind)).toEqual(["card", "card"]);
    expect(graph.edges.map((e) => [e.from, e.to])).toEqual([[START, 1], [1, 2], [2, END]]);
  });

  it("is coloured and worded by what the review is doing", () => {
    expect(reviewToneOf("in_review")).toBe("review");
    expect(reviewToneOf("integrated")).toBe("done");
    expect(reviewToneOf("working")).toBe("idle");
    expect(reviewLabel({ state: "in_review", returned_count: 0 })).toBe("being reviewed");
    expect(reviewLabel({ state: "verified", returned_count: 1 })).toBe("abgezeichnet");
    expect(reviewLabel({ state: "working", returned_count: 2 })).toBe("returned (2×)");
  });
});

describe("the end of a plan", () => {
  it("collects the lines of every node nothing waits for, right of the last column", () => {
    // 2 and 3 both end the plan; 1 has followers and does not.
    const graph = layoutGraph([card(1), card(2, [1]), card(3, [1])]);
    expect(graph.edges.filter((e) => e.to === END).map((e) => e.from).sort()).toEqual([2, 3]);
    const lastRight = Math.max(...graph.nodes.map((n) => n.x + NODE_W));
    expect(graph.end!.x).toBeGreaterThan(lastRight);
    expect(graph.end!.x + graph.end!.w).toBeLessThanOrEqual(graph.width);
  });

  it("is open until every card that counts is integrated, then ready, and closed with the plan", () => {
    const open = endOf([{ state: "integrated" }, { state: "working" }], "approved");
    expect(open).toEqual({ tone: "idle", label: "offen" });
    expect(endOf([{ state: "integrated" }, { state: "canceled" }], "approved").label).toBe("ready to take over");
    expect(endOf([{ state: "failed" }, { state: "integrated" }], "approved").tone).toBe("failed");
    expect(endOf([], "approved").label).toBe("offen");
    expect(endOf([{ state: "taken_over" }], "closed")).toEqual({ tone: "done", label: "closed" });
  });
});
