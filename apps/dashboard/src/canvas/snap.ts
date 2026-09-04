/**
 * Tile placement rules, pure and testable:
 *
 * - grid snapping (`GRID` px, matches `--ax-grid`);
 * - magnetic edges: while moving / resizing, an edge within `MAGNET_PX` of a
 *   neighbour's edge sticks to it — touching (my left ↔ its right, …) or
 *   aligning (my left ↔ its left, …); a neighbour beats the grid;
 * - no overlap on commit: only the dropped tile moves, by the smallest push
 *   that clears every neighbour (deterministic tie order), bounded passes,
 *   then a bounded grid spiral;
 * - viewport clamping so tiles stay visible after a window resize.
 */

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Size {
  w: number;
  h: number;
}

export interface Guide {
  axis: "x" | "y";
  /** Canvas coordinate of the snapped edge. */
  at: number;
}

export const GRID = 16;
export const MAGNET_PX = 8;
/** Tolerance for the "edges line up" visual guides — independent of, and
 *  wider-reaching than, the 8px magnet (which only fires between adjacent
 *  tiles). */
export const ALIGN_PX = 3;
const MAX_PUSH_PASSES = 8;
const SPIRAL_STEPS = 400;

export function snapToGrid(v: number, grid = GRID): number {
  const r = Math.round(v / grid) * grid;
  return r === 0 ? 0 : r; // never -0
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

/** Neighbours whose projection on the other axis overlaps (or nearly). */
function nearOnAxis(rect: Rect, o: Rect, axis: "x" | "y", magnet: number): boolean {
  return axis === "x"
    ? rect.y < o.y + o.h + magnet && rect.y + rect.h > o.y - magnet
    : rect.x < o.x + o.w + magnet && rect.x + rect.w > o.x - magnet;
}

interface Candidate {
  delta: number;
  touch: boolean;
  at: number;
  index: number;
}

function best(cands: Candidate[], magnet: number): Candidate | null {
  let pick: Candidate | null = null;
  for (const c of cands) {
    if (Math.abs(c.delta) > magnet) continue;
    if (
      !pick ||
      Math.abs(c.delta) < Math.abs(pick.delta) ||
      (Math.abs(c.delta) === Math.abs(pick.delta) && ((c.touch && !pick.touch) || (c.touch === pick.touch && c.index < pick.index)))
    ) {
      pick = c;
    }
  }
  return pick;
}

export interface MagnetOptions {
  grid?: number;
  magnet?: number;
  /** Set false to disable neighbour snapping (grid only). */
  edges?: boolean;
}

/** Snapped position for a tile being moved. */
export function magnetMove(rect: Rect, others: Rect[], opts: MagnetOptions = {}): { x: number; y: number; guides: Guide[] } {
  const grid = opts.grid ?? GRID;
  const magnet = opts.magnet ?? MAGNET_PX;
  const guides: Guide[] = [];
  let x = snapToGrid(rect.x, grid);
  let y = snapToGrid(rect.y, grid);
  if (opts.edges !== false) {
    const xs: Candidate[] = [];
    const ys: Candidate[] = [];
    others.forEach((o, index) => {
      if (nearOnAxis(rect, o, "x", magnet)) {
        xs.push({ delta: o.x + o.w - rect.x, touch: true, at: o.x + o.w, index }); // my left ↔ its right
        xs.push({ delta: o.x - (rect.x + rect.w), touch: true, at: o.x, index }); // my right ↔ its left
        xs.push({ delta: o.x - rect.x, touch: false, at: o.x, index }); // left ↔ left
        xs.push({ delta: o.x + o.w - (rect.x + rect.w), touch: false, at: o.x + o.w, index }); // right ↔ right
      }
      if (nearOnAxis(rect, o, "y", magnet)) {
        ys.push({ delta: o.y + o.h - rect.y, touch: true, at: o.y + o.h, index });
        ys.push({ delta: o.y - (rect.y + rect.h), touch: true, at: o.y, index });
        ys.push({ delta: o.y - rect.y, touch: false, at: o.y, index });
        ys.push({ delta: o.y + o.h - (rect.y + rect.h), touch: false, at: o.y + o.h, index });
      }
    });
    const bx = best(xs, magnet);
    const by = best(ys, magnet);
    if (bx) {
      x = rect.x + bx.delta;
      guides.push({ axis: "x", at: bx.at });
    }
    if (by) {
      y = rect.y + by.delta;
      guides.push({ axis: "y", at: by.at });
    }
  }
  return { x: Math.max(0, x), y: Math.max(0, y), guides };
}

/** Snapped size for a tile being resized from its right / bottom edges. */
export function magnetResize(
  rect: Rect,
  others: Rect[],
  dir: "e" | "s" | "se",
  min: Size,
  opts: MagnetOptions = {},
): { w: number; h: number; guides: Guide[] } {
  const grid = opts.grid ?? GRID;
  const magnet = opts.magnet ?? MAGNET_PX;
  const guides: Guide[] = [];
  let w = dir === "s" ? rect.w : snapToGrid(rect.w, grid);
  let h = dir === "e" ? rect.h : snapToGrid(rect.h, grid);
  if (opts.edges !== false) {
    if (dir !== "s") {
      const cands: Candidate[] = [];
      others.forEach((o, index) => {
        if (!nearOnAxis(rect, o, "x", magnet)) return;
        cands.push({ delta: o.x - (rect.x + rect.w), touch: true, at: o.x, index });
        cands.push({ delta: o.x + o.w - (rect.x + rect.w), touch: false, at: o.x + o.w, index });
      });
      const b = best(cands, magnet);
      if (b) {
        w = rect.w + b.delta;
        guides.push({ axis: "x", at: b.at });
      }
    }
    if (dir !== "e") {
      const cands: Candidate[] = [];
      others.forEach((o, index) => {
        if (!nearOnAxis(rect, o, "y", magnet)) return;
        cands.push({ delta: o.y - (rect.y + rect.h), touch: true, at: o.y, index });
        cands.push({ delta: o.y + o.h - (rect.y + rect.h), touch: false, at: o.y + o.h, index });
      });
      const b = best(cands, magnet);
      if (b) {
        h = rect.h + b.delta;
        guides.push({ axis: "y", at: b.at });
      }
    }
  }
  return { w: Math.max(min.w, w), h: Math.max(min.h, h), guides };
}

/** Figma-style alignment hints: a guide wherever the moved rect shares an
 *  edge or centre line with another tile (within `tol`), regardless of how
 *  far apart they are. Purely visual — it does not move anything. */
export function alignmentGuides(moved: Rect, others: Rect[], tol = ALIGN_PX): Guide[] {
  const out: Guide[] = [];
  const seen = new Set<string>();
  const add = (axis: "x" | "y", at: number) => {
    const key = `${axis}:${Math.round(at)}`;
    if (seen.has(key)) return;
    seen.add(key);
    out.push({ axis, at: Math.round(at) });
  };
  const mx = [moved.x, moved.x + moved.w / 2, moved.x + moved.w];
  const my = [moved.y, moved.y + moved.h / 2, moved.y + moved.h];
  for (const o of others) {
    const ox = [o.x, o.x + o.w / 2, o.x + o.w];
    const oy = [o.y, o.y + o.h / 2, o.y + o.h];
    for (const a of mx) for (const b of ox) if (Math.abs(a - b) <= tol) add("x", b);
    for (const a of my) for (const b of oy) if (Math.abs(a - b) <= tol) add("y", b);
  }
  return out;
}

/** Moves `rect` (never the others) until it overlaps nobody. */
export function resolveOverlap(rect: Rect, others: Rect[], bounds?: Size): Rect {
  let r = { ...rect };
  const inside = (c: Rect) =>
    c.x >= 0 && c.y >= 0 && (!bounds || (c.x + c.w <= bounds.w && c.y + c.h <= bounds.h));
  const clear = (c: Rect) => others.every((o) => !overlaps(c, o));
  for (let pass = 0; pass < MAX_PUSH_PASSES && !clear(r); pass++) {
    const hit = others.find((o) => overlaps(r, o))!;
    const moves: Rect[] = [
      { ...r, x: hit.x + hit.w }, // right
      { ...r, y: hit.y + hit.h }, // down
      { ...r, x: hit.x - r.w }, // left
      { ...r, y: hit.y - r.h }, // up
    ];
    const ranked = moves
      .map((m, i) => ({ m, i, d: Math.abs(m.x - r.x) + Math.abs(m.y - r.y) }))
      .filter(({ m }) => inside(m))
      .sort((a, b) => a.d - b.d || a.i - b.i);
    if (ranked.length === 0) break;
    r = ranked[0].m;
  }
  if (clear(r) && inside(r)) return r;
  // Bounded spiral over grid positions around the requested spot.
  for (let step = 1; step <= SPIRAL_STEPS; step++) {
    const ring = Math.ceil(step / 8);
    const dir = step % 8;
    const dx = [1, 1, 0, -1, -1, -1, 0, 1][dir] * ring * GRID;
    const dy = [0, 1, 1, 1, 0, -1, -1, -1][dir] * ring * GRID;
    const c = { ...rect, x: rect.x + dx, y: rect.y + dy };
    if (inside(c) && clear(c)) return c;
  }
  return rect;
}

/** Keeps a tile inside `bounds`, shrinking only if larger than the canvas. */
export function clampToBounds(rect: Rect, bounds: Size, min: Size): Rect {
  const w = Math.max(min.w, Math.min(rect.w, bounds.w));
  const h = Math.max(min.h, Math.min(rect.h, bounds.h));
  const x = Math.max(0, Math.min(rect.x, bounds.w - w));
  const y = Math.max(0, Math.min(rect.y, bounds.h - h));
  return { x, y, w, h };
}

export interface Anchor {
  x: "left" | "right";
  y: "top" | "bottom";
  w: number;
  h: number;
}

/** The edges a tile should follow: whichever is nearer to its centre. */
export function anchorFor(rect: Rect, bounds: Size): Anchor {
  const cx = rect.x + rect.w / 2;
  const cy = rect.y + rect.h / 2;
  return {
    x: bounds.w > 0 && cx > bounds.w / 2 ? "right" : "left",
    y: bounds.h > 0 && cy > bounds.h / 2 ? "bottom" : "top",
    w: bounds.w,
    h: bounds.h,
  };
}

/** Where a tile is drawn for the current canvas size: its committed
 *  position shifted with the anchored edge (right/bottom tiles track the
 *  edge), then clamped into view. Never persisted — growing the window
 *  brings every tile back to its committed spot. */
export function displayRect(rect: Rect, anchor: Anchor | undefined, bounds: Size, min: Size): Rect {
  if (bounds.w <= 0 || bounds.h <= 0) return rect;
  let { x, y } = rect;
  if (anchor) {
    if (anchor.x === "right" && anchor.w > 0) x += bounds.w - anchor.w;
    if (anchor.y === "bottom" && anchor.h > 0) y += bounds.h - anchor.h;
  }
  return clampToBounds({ x, y, w: rect.w, h: rect.h }, bounds, min);
}
