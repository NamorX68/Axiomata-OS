import { describe, expect, it } from "vitest";

import type { BoardCard, TaskState } from "../core/backend";
import { NODE_H, NODE_W, clip, layersOf, layoutGraph, toneOf } from "./flowGraph";

function card(id: number, depends_on: number[] = [], state: TaskState = "ready"): BoardCard {
  return { id, depends_on, state, title: `c${id}` } as BoardCard;
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
    expect(layoutGraph([])).toEqual({ nodes: [], edges: [], width: 0, height: 0 });
  });

  it("gives each card a place, a line for each edge, and room for all of them", () => {
    const graph = layoutGraph([card(1), card(2, [1]), card(3, [1]), card(4, [2, 3])]);
    expect(graph.nodes).toHaveLength(4);
    expect(graph.edges.map((e) => [e.from, e.to])).toEqual([[1, 2], [1, 3], [2, 4], [3, 4]]);
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
    const [edge] = graph.edges;
    const [from, to] = [graph.nodes[0], graph.nodes[1]];
    expect(edge.path.startsWith(`M ${from.x + NODE_W} ${from.y + NODE_H / 2}`)).toBe(true);
    expect(edge.path.endsWith(`${to.x} ${to.y + NODE_H / 2}`)).toBe(true);
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
