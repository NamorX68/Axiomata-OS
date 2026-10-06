/**
 * The Flow's graph of a plan (A2A CP-A10, A6): the cards as nodes, "needs first" as lines, the state as colour, laid out
 * in columns by how many cards must be done before. Pure data and geometry, drawn by `GraphPane.svelte`; no library — a
 * plan has tens of cards and its edges point one way, so a layered layout with a barycentre pass is all it takes.
 */
import type { BoardCard, TaskState } from "../core/backend";

export const NODE_W = 210;
export const NODE_H = 58;
const COL_GAP = 72;
const ROW_GAP = 16;
const PAD = 16;

export interface GraphNode {
  card: BoardCard;
  /** How many cards are, at the longest, before this one: its column. */
  layer: number;
  x: number;
  y: number;
}

export interface GraphEdge {
  from: number;
  to: number;
  /** An SVG path from the right side of `from` to the left side of `to`. */
  path: string;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  width: number;
  height: number;
}

/**
 * The column of every card: 0 for a card that needs none of the others, else one more than the deepest card it needs.
 * Edges to cards outside `ids` are ignored (a card moved to another plan). The dependencies cannot form a cycle — the
 * board refuses one — but a cycle that got in would not hang this: a card is placed at most as deep as there are cards.
 */
export function layersOf(cards: Pick<BoardCard, "id" | "depends_on">[]): Map<number, number> {
  const ids = new Set(cards.map((card) => card.id));
  const needs = new Map(cards.map((card) => [card.id, card.depends_on.filter((id) => ids.has(id))]));
  const layer = new Map<number, number>();
  const depth = (id: number, seen: number): number => {
    const known = layer.get(id);
    if (known !== undefined) return known;
    // A path longer than the number of cards can only be a cycle: stop it where it is.
    if (seen > cards.length) return 0;
    const before = needs.get(id) ?? [];
    const value = before.length === 0 ? 0 : 1 + Math.max(...before.map((dep) => depth(dep, seen + 1)));
    layer.set(id, value);
    return value;
  };
  for (const card of cards) depth(card.id, 0);
  return layer;
}

/** Orders the cards inside each column by the average row of the cards they need, so the lines cross as little as a cheap pass can. */
function orderColumns(columns: BoardCard[][]): void {
  const rowOf = new Map<number, number>();
  columns[0]?.forEach((card, row) => rowOf.set(card.id, row));
  for (let col = 1; col < columns.length; col++) {
    const barycentre = (card: BoardCard): number => {
      const rows = card.depends_on.map((id) => rowOf.get(id)).filter((row): row is number => row !== undefined);
      return rows.length === 0 ? Number.MAX_SAFE_INTEGER : rows.reduce((a, b) => a + b, 0) / rows.length;
    };
    columns[col].sort((a, b) => barycentre(a) - barycentre(b) || a.id - b.id);
    columns[col].forEach((card, row) => rowOf.set(card.id, row));
  }
}

/** The graph of `cards`: positions, and one curve per "needs first" line between two of them. */
export function layoutGraph(cards: BoardCard[]): Graph {
  if (cards.length === 0) return { nodes: [], edges: [], width: 0, height: 0 };
  const layers = layersOf(cards);
  const depth = Math.max(...layers.values());
  const columns: BoardCard[][] = Array.from({ length: depth + 1 }, () => []);
  for (const card of [...cards].sort((a, b) => a.id - b.id)) columns[layers.get(card.id) ?? 0].push(card);
  orderColumns(columns);

  const nodes: GraphNode[] = [];
  columns.forEach((column, layer) =>
    column.forEach((card, row) =>
      nodes.push({
        card,
        layer,
        x: PAD + layer * (NODE_W + COL_GAP),
        y: PAD + row * (NODE_H + ROW_GAP),
      }),
    ),
  );
  const at = new Map(nodes.map((node) => [node.card.id, node]));
  const edges: GraphEdge[] = [];
  for (const node of nodes) {
    for (const dep of node.card.depends_on) {
      const from = at.get(dep);
      if (!from) continue;
      const x1 = from.x + NODE_W;
      const y1 = from.y + NODE_H / 2;
      const x2 = node.x;
      const y2 = node.y + NODE_H / 2;
      const bend = (x2 - x1) / 2;
      edges.push({ from: dep, to: node.card.id, path: `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}` });
    }
  }
  const widest = Math.max(...columns.map((column) => column.length));
  return {
    nodes,
    edges,
    width: PAD * 2 + columns.length * NODE_W + (columns.length - 1) * COL_GAP,
    height: PAD * 2 + widest * NODE_H + (widest - 1) * ROW_GAP,
  };
}

/** How a state is drawn: one class per family, so the colours stay in the stylesheet and the tokens. */
export type Tone = "idle" | "active" | "attention" | "review" | "done" | "failed";

export function toneOf(state: TaskState): Tone {
  switch (state) {
    case "working":
      return "active";
    case "input_required":
      return "attention";
    case "in_review":
      return "review";
    case "verified":
    case "integrated":
    case "done":
    case "taken_over":
      return "done";
    case "failed":
    case "canceled":
      return "failed";
    default:
      return "idle";
  }
}

/** A title cut to what fits a node. */
export function clip(text: string, max = 27): string {
  const line = text.split("\n")[0].trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}
