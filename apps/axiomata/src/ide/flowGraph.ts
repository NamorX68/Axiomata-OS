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
export const START_W = 64;
export const START_H = 36;

export interface GraphNode {
  card: BoardCard;
  /** How many cards are, at the longest, before this one: its column. */
  layer: number;
  x: number;
  y: number;
}

/** The id the start node has in an edge's `from`. */
export const START = 0;

/** The point the tree grows from: a small node left of the first column. */
export interface StartNode {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface GraphEdge {
  /** The card the line leaves, or [`START`] for a card that needs none. */
  from: number;
  to: number;
  /** An SVG path from the right side of `from` to the left side of `to`. */
  path: string;
}

export interface Graph {
  /** `null` for no cards. */
  start: StartNode | null;
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

/** The graph of `cards`: a start node, positions, and one curve per "needs first" line — and per card that needs none, one from the start. */
export function layoutGraph(cards: BoardCard[]): Graph {
  if (cards.length === 0) return { start: null, nodes: [], edges: [], width: 0, height: 0 };
  const layers = layersOf(cards);
  const depth = Math.max(...layers.values());
  const columns: BoardCard[][] = Array.from({ length: depth + 1 }, () => []);
  for (const card of [...cards].sort((a, b) => a.id - b.id)) columns[layers.get(card.id) ?? 0].push(card);
  orderColumns(columns);

  const widest = Math.max(...columns.map((column) => column.length));
  const heightOf = (rows: number, row: number) => rows * row + (rows - 1) * ROW_GAP;
  const inner = Math.max(heightOf(widest, NODE_H), START_H);
  const height = PAD * 2 + inner;
  const firstX = PAD + START_W + COL_GAP;

  const nodes: GraphNode[] = [];
  columns.forEach((column, layer) => {
    // Each column hangs around the middle of the tallest one, so the tree grows out of the centre line.
    const top = PAD + (inner - heightOf(column.length, NODE_H)) / 2;
    column.forEach((card, row) =>
      nodes.push({
        card,
        layer,
        x: firstX + layer * (NODE_W + COL_GAP),
        y: top + row * (NODE_H + ROW_GAP),
      }),
    );
  });
  const start: StartNode = { x: PAD, y: PAD + (inner - START_H) / 2, w: START_W, h: START_H };

  const at = new Map(nodes.map((node) => [node.card.id, node]));
  const curve = (x1: number, y1: number, x2: number, y2: number): string => {
    const bend = (x2 - x1) / 2;
    return `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`;
  };
  const edges: GraphEdge[] = [];
  for (const node of nodes) {
    const sources = node.card.depends_on.filter((dep) => at.has(dep));
    if (sources.length === 0) {
      edges.push({
        from: START,
        to: node.card.id,
        path: curve(start.x + start.w, start.y + start.h / 2, node.x, node.y + NODE_H / 2),
      });
    }
    for (const dep of sources) {
      const from = at.get(dep)!;
      edges.push({
        from: dep,
        to: node.card.id,
        path: curve(from.x + NODE_W, from.y + NODE_H / 2, node.x, node.y + NODE_H / 2),
      });
    }
  }
  return {
    start,
    nodes,
    edges,
    width: firstX + columns.length * NODE_W + (columns.length - 1) * COL_GAP + PAD,
    height,
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
