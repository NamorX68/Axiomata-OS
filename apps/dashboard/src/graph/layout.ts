/**
 * Ring layout (the reference look): hub at the centre, skills on an inner
 * ring, files on concentric arcs inside their area's angular segment,
 * routines on the outer ring. Positions are in graph units where 1 = the
 * outer radius; the renderer scales to the canvas.
 */

import type { GraphModel, GraphNode } from "./model";

export const RING = {
  skills: 0.17,
  areas: 0.3,
  filesInner: 0.4,
  filesOuter: 0.84,
  routines: 0.95,
} as const;

/** Arc spacing between file dots in graph units (at radius r the dot
 *  spacing along the arc is this). */
const ARC_STEP = 0.028;
const RING_STEP = 0.032;

function placeRing(nodes: GraphNode[], radius: number, startAngle = -Math.PI / 2): void {
  const n = nodes.length;
  nodes.forEach((node, i) => {
    const a = startAngle + (i / Math.max(n, 1)) * Math.PI * 2;
    node.x = Math.cos(a) * radius;
    node.y = Math.sin(a) * radius;
  });
}

export type LayoutKind = "rings" | "circle" | "hex";

/** Most icon nodes on the dashboard orbit ring. */
export const ORBIT_MAX = 36;

/** Dashboard-centre layout (the reference look): skills, routines and the
 *  most recently changed notes as icon nodes on the outer ring; every file
 *  as a point of a 3-D cloud (fibonacci sphere, denser towards the centre)
 *  that the renderer spins and projects. */
export function layoutOrbit(model: GraphModel): void {
  const hub = model.byId.get("hub");
  if (hub) {
    hub.x = 0;
    hub.y = 0;
  }
  const files = model.nodes.filter((n) => n.kind === "file");
  const recent = [...files]
    .filter((n) => n.modified)
    .sort((a, b) => Date.parse(b.modified!) - Date.parse(a.modified!));
  const ring = [
    ...model.nodes.filter((n) => n.kind === "skill"),
    ...model.nodes.filter((n) => n.kind === "routine"),
  ];
  for (const n of recent) {
    if (ring.length >= ORBIT_MAX) break;
    ring.push(n);
  }
  for (const n of model.nodes) n.onOrbit = false;
  ring.forEach((n, i) => {
    n.onOrbit = true;
    const a = -Math.PI / 2 + (i / ring.length) * Math.PI * 2;
    n.x = Math.cos(a);
    n.y = Math.sin(a);
  });
  // Areas are not drawn in orbit mode; park them at the centre.
  for (const n of model.nodes.filter((n) => n.kind === "area")) {
    n.x = 0;
    n.y = 0;
  }
  // Cloud: fibonacci sphere, radius by cube-root so the blob is solid.
  const golden = Math.PI * (3 - Math.sqrt(5));
  const total = files.length || 1;
  files.forEach((n, i) => {
    const t = (i + 0.5) / total;
    const yUnit = 1 - 2 * t;
    const rUnit = Math.sqrt(Math.max(0, 1 - yUnit * yUnit));
    const theta = golden * i;
    const radius = 0.62 * Math.cbrt(0.25 + 0.75 * ((n.phase + 0.5) % 1));
    n.p3 = [Math.cos(theta) * rUnit * radius, yUnit * radius, Math.sin(theta) * rUnit * radius];
  });
}

export function applyLayout(model: GraphModel, kind: LayoutKind): void {
  if (kind === "hex") layoutHex(model);
  else if (kind === "circle") layoutCircle(model);
  else layoutRings(model);
}

/** All files on one ring, grouped by area segment; skills / routines as in
 *  the ring layout. Good for spotting links. */
export function layoutCircle(model: GraphModel): void {
  layoutRings(model);
  const files = model.nodes.filter((n) => n.kind === "file");
  const radius = (RING.filesInner + RING.filesOuter) / 2 + 0.1;
  for (const seg of model.areas) {
    const mine = files
      .filter((n) => n.area === seg.name)
      .sort((a, b) => a.label.localeCompare(b.label));
    const span = seg.end - seg.start;
    mine.forEach((node, i) => {
      const a = mine.length > 1 ? seg.start + (i / (mine.length - 1)) * span : (seg.start + seg.end) / 2;
      node.x = Math.cos(a) * radius;
      node.y = Math.sin(a) * radius;
    });
  }
}

export function layoutRings(model: GraphModel): void {
  const hub = model.byId.get("hub");
  if (hub) {
    hub.x = 0;
    hub.y = 0;
  }
  placeRing(
    model.nodes.filter((n) => n.kind === "skill"),
    RING.skills,
    -Math.PI / 2 + 0.6,
  );
  placeRing(
    model.nodes.filter((n) => n.kind === "routine"),
    RING.routines,
    -Math.PI / 2 + 0.45,
  );
  // Area nodes sit at the middle angle of their own segment.
  for (const seg of model.areas) {
    const node = model.byId.get(`area:${seg.name}`);
    if (!node) continue;
    const a = (seg.start + seg.end) / 2;
    node.x = Math.cos(a) * RING.areas;
    node.y = Math.sin(a) * RING.areas;
  }

  // Files: per area, fill successive arcs from the inner radius outwards.
  const files = model.nodes.filter((n) => n.kind === "file");
  const loose = files.filter((n) => n.area === null);
  for (const seg of model.areas) {
    const mine = files
      .filter((n) => n.area === seg.name)
      .sort((a, b) => a.label.localeCompare(b.label));
    const span = seg.end - seg.start;
    // First pass: how many arcs does this segment need at the natural
    // spacing? Then spread those arcs evenly across the file band so a
    // small vault still fills the ring area instead of huddling inside.
    const rows: GraphNode[][] = [];
    let radius: number = RING.filesInner;
    let i = 0;
    while (i < mine.length) {
      const perArc = Math.max(1, Math.floor((span * radius) / ARC_STEP));
      rows.push(mine.slice(i, i + perArc));
      i += perArc;
      radius = Math.min(RING.filesOuter, radius + RING_STEP);
    }
    const naturalSpan = (rows.length - 1) * RING_STEP;
    const bandSpan = RING.filesOuter - RING.filesInner;
    const stepR = rows.length > 1 ? Math.max(RING_STEP, bandSpan / (rows.length - 1)) : 0;
    const startR = rows.length > 1 && naturalSpan < bandSpan ? RING.filesInner : RING.filesInner;
    rows.forEach((row, k) => {
      const r = rows.length === 1 ? (RING.filesInner + RING.filesOuter) / 2 : Math.min(RING.filesOuter, startR + k * stepR);
      const step = row.length > 1 ? span / (row.length - 1) : 0;
      row.forEach((node, j) => {
        const a = row.length > 1 ? seg.start + j * step : (seg.start + seg.end) / 2;
        node.x = Math.cos(a) * r;
        node.y = Math.sin(a) * r;
      });
    });
  }
  // Loose root files sit on a small ring between skills and the areas.
  placeRing(loose, (RING.skills + RING.filesInner) / 2, Math.PI / 2);
}

/** The 6 axial unit directions, in ring-walking order — convention-
 *  independent of flat- vs pointy-top; only `hexToPixel` cares about that. */
const AXIAL_DIRS: [number, number][] = [
  [1, 0],
  [1, -1],
  [0, -1],
  [-1, 0],
  [-1, 1],
  [0, 1],
];

/** Flat-top axial → pixel, hex circumradius `u` (graph units). */
function hexToPixel(q: number, r: number, u: number): [number, number] {
  return [u * 1.5 * q, u * Math.sqrt(3) * (r + q / 2)];
}

/** Every axial cell on hex ring `k` (k ≥ 1) around the origin, walked side
 *  by side — the standard "spiral ring" traversal. */
function hexRing(k: number): [number, number][] {
  const cells: [number, number][] = [];
  let q = AXIAL_DIRS[4][0] * k;
  let r = AXIAL_DIRS[4][1] * k;
  for (const [dq, dr] of AXIAL_DIRS) {
    for (let step = 0; step < k; step++) {
      cells.push([q, r]);
      q += dq;
      r += dr;
    }
  }
  return cells;
}

/** Is `angle` inside `[start, end)`, both possibly outside ±π (area
 *  segments are laid out as a running sum, not wrapped)? */
function angleInSegment(angle: number, start: number, end: number): boolean {
  const twoPi = Math.PI * 2;
  let a = angle;
  while (a < start) a += twoPi;
  while (a >= start + twoPi) a -= twoPi;
  return a >= start && a < end;
}

/** Honeycomb layout: hub / skills / routines / area markers exactly as in
 *  Rings (so the centre still reads the same way); every file gets its own
 *  hex cell instead of a dot, tiled as a true flat-top hex grid (a spiral
 *  of rings out from the origin, per `hexRing`) so neighbouring notes touch
 *  edge-to-edge like a honeycomb. Cells are handed out per area, in the
 *  same angular wedge each area already owns in Rings/Circle, so the
 *  overall shape still reads as "areas around a hub" — just made of hex
 *  tiles instead of arcs of dots. One shared cell size (`model.hexUnit`,
 *  graph units) is solved from the file count so the mosaic fills roughly
 *  the same band Rings uses for files, then everything is rescaled to
 *  land exactly on that band's outer edge; the renderer reads `hexUnit`
 *  back to draw each cell at the matching pixel size. */
export function layoutHex(model: GraphModel): void {
  layoutRings(model);
  const files = model.nodes.filter((n) => n.kind === "file");
  if (files.length === 0) {
    model.hexUnit = undefined;
    return;
  }
  // Solve one hex circumradius `u` so `files.length` hexes, each of area
  // (3√3/2)u², cover roughly the annulus Rings uses for files (with a
  // little slack so the outermost ring isn't razor-tight against neighbours).
  // The mosaic starts a bit further out than Rings' dots do — otherwise the
  // innermost cells crowd right up against the skill/routine/area icons and
  // make them hard to pick out.
  const innerR = RING.filesInner + 0.32;
  const outerR = RING.filesOuter;
  const slack = 1.15;
  const fieldArea = Math.PI * (outerR * outerR - innerR * innerR);
  const hexArea = (3 * Math.sqrt(3)) / 2;
  let u = Math.sqrt((fieldArea * slack) / (files.length * hexArea));

  const ringHeight = Math.sqrt(3) * u;
  const kStart = Math.max(1, Math.ceil(innerR / ringHeight));

  interface Cell {
    x: number;
    y: number;
    ring: number;
    angle: number;
  }
  const genCells = (unit: number): Cell[] => {
    const out: Cell[] = [];
    let k = kStart;
    while (out.length < files.length * 1.3 && k < kStart + 200) {
      for (const [q, r] of hexRing(k)) {
        const [x, y] = hexToPixel(q, r, unit);
        out.push({ x, y, ring: k, angle: Math.atan2(y, x) });
      }
      k++;
    }
    return out;
  };
  let cells = genCells(u);

  // Loose (no-area) files don't get a proper wedge in Rings either — give
  // them a narrow slice pointing at the same spot Rings parks their ring.
  const wedges = [
    ...model.areas.map((s) => ({ area: s.name, start: s.start, end: s.end })),
    { area: null, start: Math.PI / 2 - 0.15, end: Math.PI / 2 + 0.15 },
  ];
  const byArea = new Map<string | null, GraphNode[]>();
  for (const n of files) {
    const key = n.area;
    if (!byArea.has(key)) byArea.set(key, []);
    byArea.get(key)!.push(n);
  }

  const assign = (cellPool: Cell[]): Set<GraphNode> => {
    const used = new Set<number>();
    const assigned = new Set<GraphNode>();
    for (const w of wedges) {
      const mine = (byArea.get(w.area) ?? []).slice().sort((a, b) => a.label.localeCompare(b.label));
      if (mine.length === 0) continue;
      const candidates = cellPool
        .map((c, i) => ({ c, i }))
        .filter(({ i, c }) => !used.has(i) && angleInSegment(c.angle, w.start, w.end))
        .sort((a, b) => a.c.ring - b.c.ring || a.c.angle - b.c.angle);
      mine.forEach((node, j) => {
        const pick = candidates[j];
        if (!pick) return;
        used.add(pick.i);
        node.x = pick.c.x;
        node.y = pick.c.y;
        assigned.add(node);
      });
    }
    // Spillover: a wedge that ran short (discretisation, not a real area
    // imbalance) — hand its leftover files the nearest still-free cells.
    const leftover = files.filter((n) => !assigned.has(n));
    if (leftover.length > 0) {
      const free = cellPool
        .map((c, i) => ({ c, i }))
        .filter(({ i }) => !used.has(i))
        .sort((a, b) => a.c.ring - b.c.ring || a.c.angle - b.c.angle);
      leftover.forEach((node, j) => {
        const pick = free[j];
        if (!pick) return;
        node.x = pick.c.x;
        node.y = pick.c.y;
        assigned.add(node);
      });
    }
    return assigned;
  };
  assign(cells);

  // Rescale so the outermost placed cell lands on `outerR` — fills the
  // same band regardless of how the ring-spiral vs. annulus area estimates
  // happened to compare, and keeps `hexUnit` in step with the final layout.
  const maxR = Math.max(...files.map((n) => Math.hypot(n.x, n.y)), 1e-6);
  const scale = outerR / maxR;
  u *= scale;
  for (const n of files) {
    n.x *= scale;
    n.y *= scale;
  }
  model.hexUnit = u;
}
