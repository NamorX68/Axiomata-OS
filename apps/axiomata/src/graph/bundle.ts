/**
 * The curves of the Bundle layout (`docs/plans/brain-views.md`, BV1): hierarchical edge bundling after Holten (2006).
 * Every note sits on one ring; a link between two notes is drawn as a B-spline whose control points are the notes'
 * ancestors in the vault's tree (note → folder … → area → root and back down), pulled a little towards the
 * straight line by the bundling strength β. Links that share ancestors share control points, so they run together
 * as bundles.
 * Pure geometry in graph units: `layoutBundle` (`graph/layout.ts`) builds the tree, the renderer draws the commands.
 */

/** A point in graph units (0, 0 = the ring's centre). */
export interface BundlePoint {
  x: number;
  y: number;
}

/** One arc of the ring: an area (or, grouped by folders, a deepest folder) and the angle its notes take. */
export interface BundleArc {
  /** The area or folder the arc stands for; `null` for the notes at the vault's root. */
  area: string | null;
  /** Radians, clockwise on screen (y points down) from `start` to `end`. */
  start: number;
  end: number;
  count: number;
}

/** The tree the curves bundle along, as `layoutBundle` laid it out. */
export interface BundleLayout {
  arcs: BundleArc[];
  /** Every tree node's control point by key: {@link BUNDLE_ROOT}, `dir:<path>` for a folder, a file node's id for a
   *  note. */
  points: Map<string, BundlePoint>;
  /** Every tree node's parent key; the root has none. */
  parent: Map<string, string>;
}

/** The key of the tree's root, the ring's centre. */
export const BUNDLE_ROOT = "";

/** The key of a folder's (or an area's) node in the tree. */
export const dirKey = (path: string): string => `dir:${path}`;

/** How strongly the curves follow the tree (Holten's β): 1 runs every curve through all its ancestors, 0 draws straight
 *  lines. 0.85 keeps the bundles while two links between the same areas stay apart. */
export const BUNDLE_BETA = 0.85;

/** How long a link counts as new, and is drawn in the glow (brain-views Q11: fresh is alive). */
export const FRESH_LINK_DAYS = 21;

const DAY_MS = 86_400_000;

/** A drawing command of a curve, in graph units — the subset of the canvas path API a B-spline needs. */
export type BundleCommand =
  | { kind: "move"; x: number; y: number }
  | { kind: "line"; x: number; y: number }
  | { kind: "curve"; x1: number; y1: number; x2: number; y2: number; x: number; y: number };

/** `key` and its ancestors up to the root, nearest first. A key the tree does not know is a chain of its own. */
function chain(layout: BundleLayout, key: string): string[] {
  const out = [key];
  let at = layout.parent.get(key);
  // Bounded by the tree's size, so a malformed parent map cannot loop forever.
  while (at !== undefined && out.length <= layout.parent.size + 1) {
    out.push(at);
    at = layout.parent.get(at);
  }
  return out;
}

/**
 * The control points of the link from `from` to `to` (two note keys): up the tree from `from` to the nearest common
 * ancestor, then down to `to` — the ancestor itself included once, as d3's `node.path` does. Empty when either note
 * has no place in the layout.
 */
export function bundleControls(layout: BundleLayout, from: string, to: string): BundlePoint[] {
  if (!layout.points.has(from) || !layout.points.has(to)) return [];
  const up = chain(layout, from);
  const down = chain(layout, to);
  const inDown = new Map(down.map((key, i) => [key, i]));
  let meet = up.findIndex((key) => inDown.has(key));
  // Two notes without a common root (not in a layout built by `layoutBundle`): straight from one to the other.
  if (meet < 0) meet = up.length - 1;
  const meetDown = inDown.get(up[meet]!) ?? down.length;
  const keys = [...up.slice(0, meet + 1), ...down.slice(0, meetDown).reverse()];
  return keys.map((key) => layout.points.get(key)).filter((p): p is BundlePoint => p !== undefined);
}

/**
 * The control points pulled towards the straight line between the first and the last by `1 − beta` (Holten's
 * bundling strength): `beta` 1 leaves them, 0 lays them all on the line.
 */
export function straighten(points: readonly BundlePoint[], beta: number): BundlePoint[] {
  const n = points.length;
  if (n <= 2) return points.map((p) => ({ x: p.x, y: p.y }));
  const first = points[0]!;
  const last = points[n - 1]!;
  return points.map((p, i) => {
    const t = i / (n - 1);
    return {
      x: beta * p.x + (1 - beta) * (first.x + t * (last.x - first.x)),
      y: beta * p.y + (1 - beta) * (first.y + t * (last.y - first.y)),
    };
  });
}

/**
 * A uniform cubic B-spline through `points`, clamped to start at the first and end at the last (the curve d3's
 * `curveBasis` draws), as canvas commands: it bends towards the control points between without passing through them.
 */
export function basisCommands(points: readonly BundlePoint[]): BundleCommand[] {
  const n = points.length;
  if (n === 0) return [];
  const p0 = points[0]!;
  const out: BundleCommand[] = [{ kind: "move", x: p0.x, y: p0.y }];
  if (n === 1) return out;
  if (n === 2) {
    out.push({ kind: "line", x: points[1]!.x, y: points[1]!.y });
    return out;
  }
  const p1 = points[1]!;
  out.push({ kind: "line", x: (5 * p0.x + p1.x) / 6, y: (5 * p0.y + p1.y) / 6 });
  const segment = (a: BundlePoint, b: BundlePoint, c: BundlePoint): BundleCommand => ({
    kind: "curve",
    x1: (2 * a.x + b.x) / 3,
    y1: (2 * a.y + b.y) / 3,
    x2: (a.x + 2 * b.x) / 3,
    y2: (a.y + 2 * b.y) / 3,
    x: (a.x + 4 * b.x + c.x) / 6,
    y: (a.y + 4 * b.y + c.y) / 6,
  });
  for (let i = 2; i < n; i++) out.push(segment(points[i - 2]!, points[i - 1]!, points[i]!));
  // The clamped end: one more segment that repeats the last point, then straight onto it.
  const a = points[n - 2]!;
  const b = points[n - 1]!;
  out.push(segment(a, b, b));
  out.push({ kind: "line", x: b.x, y: b.y });
  return out;
}

/** The whole curve of a link, ready to draw: its control points, straightened by `beta`, as a B-spline. */
export function bundleCurve(layout: BundleLayout, from: string, to: string, beta = BUNDLE_BETA): BundleCommand[] {
  return basisCommands(straighten(bundleControls(layout, from, to), beta));
}

/**
 * When a link came into being, as far as the vault can tell: a link has no date of its own (the scan sees only the
 * notes), but it cannot be older than the younger of its two notes — so that is its birth. `null` when neither note
 * has a time.
 */
export function linkBorn(fromBorn: number | null, toBorn: number | null): number | null {
  if (fromBorn === null) return toBorn;
  if (toBorn === null) return fromBorn;
  return Math.max(fromBorn, toBorn);
}

/** Whether a link born at `born` is new at `now` (younger than {@link FRESH_LINK_DAYS}). */
export function isFreshLink(born: number | null, now: number): boolean {
  if (born === null) return false;
  const age = now - born;
  return age >= 0 && age < FRESH_LINK_DAYS * DAY_MS;
}
