/**
 * The Flow's graph of a plan (A2A CP-A10, A6): the cards as nodes, "needs first" as lines, the state as colour, laid out
 * in columns by how many cards must be done before. Pure data and geometry, drawn by `GraphPane.svelte`; no library — a
 * plan has tens of cards and its edges point one way, so a layered layout with a barycentre pass is all it takes.
 */
import type { BoardCard, TaskState } from "../core/backend";

export const NODE_W = 300;
export const NODE_H = 78;
const COL_GAP = 80;
const ROW_GAP = 16;
const PAD = 16;
export const START_W = 64;
export const START_H = 36;
export const END_W = 150;
export const END_H = 52;

export interface GraphNode {
  /** The node's id in an edge: the card's id, or minus the card's id for the review stage after it. */
  key: number;
  card: BoardCard;
  /** A card of the plan, or the review that comes after it (drawn as a node of its own). */
  kind: "card" | "review";
  /** How many cards are, at the longest, before this one: its column. */
  layer: number;
  x: number;
  y: number;
}

/** The id the start node has in an edge's `from`. */
export const START = 0;

/** The id the end node has in an edge's `to`: no card or review has it. */
export const END = Number.MAX_SAFE_INTEGER;

/** The point the tree grows from: a small node left of the first column. */
export interface StartNode {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface GraphEdge {
  /** The node the line leaves ([`GraphNode.key`]), or [`START`] for a card that needs none. */
  from: number;
  to: number;
  /** An SVG path from the right side of `from` to the left side of `to`. */
  path: string;
}

export interface Graph {
  /** `null` for no cards. */
  start: StartNode | null;
  /** Where the lines of every node that nothing waits for come together: the plan's goal. `null` for no cards. */
  end: StartNode | null;
  nodes: GraphNode[];
  edges: GraphEdge[];
  width: number;
  height: number;
}

/** The states in which a card has a review stage: it was handed in, and is judged or was. */
const REVIEWED = new Set<TaskState>(["in_review", "verified", "integrated", "taken_over"]);

/**
 * Whether the card shows a review node after it: it is in review, was signed off, or was sent back at least once (a
 * card
 * returned to its worker has had a review even though it is "working" again).
 */
export function hasReviewStage(card: Pick<BoardCard, "state" | "returned_count">): boolean {
  return REVIEWED.has(card.state) || card.returned_count > 0;
}

/** What the layout places: a card, or the review after it. `id` is the node's key, `depends_on` the keys it waits
 * for. */
interface Item {
  id: number;
  depends_on: number[];
  card: BoardCard;
  kind: "card" | "review";
}

/** The items of `cards`: each card, and after a card with a review stage its review node, which the cards that need
 * it wait for instead. */
function itemsOf(cards: BoardCard[]): Item[] {
  const reviewed = new Set(cards.filter(hasReviewStage).map((card) => card.id));
  const items: Item[] = [];
  for (const card of cards) {
    const depends_on = card.depends_on.map((dep) => (reviewed.has(dep) ? -dep : dep));
    items.push({ id: card.id, depends_on, card, kind: "card" });
    if (reviewed.has(card.id)) items.push({ id: -card.id, depends_on: [card.id], card, kind: "review" });
  }
  return items;
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
function orderColumns(columns: Item[][]): void {
  const rowOf = new Map<number, number>();
  columns[0]?.forEach((card, row) => rowOf.set(card.id, row));
  for (let col = 1; col < columns.length; col++) {
    const barycentre = (card: Item): number => {
      const rows = card.depends_on.map((id) => rowOf.get(id)).filter((row): row is number => row !== undefined);
      return rows.length === 0 ? Number.MAX_SAFE_INTEGER : rows.reduce((a, b) => a + b, 0) / rows.length;
    };
    columns[col].sort((a, b) => barycentre(a) - barycentre(b) || a.id - b.id);
    columns[col].forEach((card, row) => rowOf.set(card.id, row));
  }
}

/** The graph of `cards`: a start node, positions, and one curve per "needs first" line — and per card that needs none, one from the start. */
export function layoutGraph(cards: BoardCard[]): Graph {
  if (cards.length === 0) return { start: null, end: null, nodes: [], edges: [], width: 0, height: 0 };
  const items = itemsOf(cards);
  const layers = layersOf(items);
  const depth = Math.max(...layers.values());
  const columns: Item[][] = Array.from({ length: depth + 1 }, () => []);
  // By the card's id, a card before its review: the review node has the card's id with the sign turned.
  const order = (item: Item): number => Math.abs(item.id) * 2 + (item.kind === "review" ? 1 : 0);
  for (const item of [...items].sort((a, b) => order(a) - order(b))) columns[layers.get(item.id) ?? 0].push(item);
  orderColumns(columns);

  const widest = Math.max(...columns.map((column) => column.length));
  const heightOf = (rows: number, row: number) => rows * row + (rows - 1) * ROW_GAP;
  const inner = Math.max(heightOf(widest, NODE_H), START_H, END_H);
  const height = PAD * 2 + inner;
  const firstX = PAD + START_W + COL_GAP;

  const nodes: GraphNode[] = [];
  columns.forEach((column, layer) => {
    // Each column hangs around the middle of the tallest one, so the tree grows out of the centre line.
    const top = PAD + (inner - heightOf(column.length, NODE_H)) / 2;
    column.forEach((item, row) =>
      nodes.push({
        key: item.id,
        card: item.card,
        kind: item.kind,
        layer,
        x: firstX + layer * (NODE_W + COL_GAP),
        y: top + row * (NODE_H + ROW_GAP),
      }),
    );
  });
  const start: StartNode = { x: PAD, y: PAD + (inner - START_H) / 2, w: START_W, h: START_H };
  const columnsEnd = firstX + columns.length * NODE_W + (columns.length - 1) * COL_GAP;
  const end: StartNode = { x: columnsEnd + COL_GAP, y: PAD + (inner - END_H) / 2, w: END_W, h: END_H };

  const at = new Map(nodes.map((node) => [node.key, node]));
  const needs = new Map(items.map((item) => [item.id, item.depends_on]));
  const curve = (x1: number, y1: number, x2: number, y2: number): string => {
    const bend = (x2 - x1) / 2;
    return `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`;
  };
  const edges: GraphEdge[] = [];
  for (const node of nodes) {
    const sources = (needs.get(node.key) ?? []).filter((dep) => at.has(dep));
    if (sources.length === 0) {
      edges.push({
        from: START,
        to: node.key,
        path: curve(start.x + start.w, start.y + start.h / 2, node.x, node.y + NODE_H / 2),
      });
    }
    for (const dep of sources) {
      const from = at.get(dep)!;
      edges.push({
        from: dep,
        to: node.key,
        path: curve(from.x + NODE_W, from.y + NODE_H / 2, node.x, node.y + NODE_H / 2),
      });
    }
  }
  // Every node that nothing waits for ends in the goal.
  const waitedFor = new Set(items.flatMap((item) => item.depends_on));
  for (const node of nodes) {
    if (waitedFor.has(node.key)) continue;
    edges.push({
      from: node.key,
      to: END,
      path: curve(node.x + NODE_W, node.y + NODE_H / 2, end.x, end.y + end.h / 2),
    });
  }
  return { start, end, nodes, edges, width: end.x + end.w + PAD, height };
}

/**
 * How the review node of a card is drawn: in review is the review's own colour, a card signed off or integrated is
 * done, a
 * card sent back and working again leaves its review waiting (idle).
 */
export function reviewToneOf(state: TaskState): Tone {
  if (state === "in_review") return "review";
  if (state === "verified" || state === "integrated" || state === "taken_over") return "done";
  return "idle";
}

/**
 * The end node of a plan: the plan is done when every card that still counts (not called off) is integrated or taken
 * over, and closed when the owner took it over (or closed it). Anything else is still open.
 */
export function endOf(
  cards: Pick<BoardCard, "state">[],
  planStatus: "draft" | "approved" | "closed",
): { tone: Tone; label: string } {
  if (planStatus === "closed") return { tone: "done", label: "abgeschlossen" };
  const counting = cards.filter((card) => card.state !== "canceled");
  const allIn =
    counting.length > 0 && counting.every((card) => card.state === "integrated" || card.state === "taken_over");
  if (allIn) return { tone: "done", label: "bereit zum Übernehmen" };
  if (counting.some((card) => card.state === "failed")) return { tone: "failed", label: "blockiert" };
  return { tone: "idle", label: "offen" };
}

/** What the review node says about its card: judged now, signed off, or sent back. */
export function reviewLabel(card: Pick<BoardCard, "state" | "returned_count">): string {
  if (card.state === "in_review") return "wird geprüft";
  if (card.state === "verified" || card.state === "integrated" || card.state === "taken_over") return "abgezeichnet";
  return card.returned_count > 0 ? `zurückgegeben (${card.returned_count}×)` : "";
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
export function clip(text: string, max = 32): string {
  const line = text.split("\n")[0].trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}
