/**
 * Where the graph's nodes sit: the Orbit's cloud and rings, and the Second Brain's layouts — Bundle (one ring of
 * notes, `layoutBundle`), Map (`layoutMap`) and Hex (`layoutHex`). Routines are not drawn (owner, 2026-10-07: they live
 * on the Routines board). Positions are in graph units where 1 = the outer radius; the renderer scales to the canvas.
 */

import type { BrainLayout } from "../core/brainView";
import { BUNDLE_ROOT, dirKey, type BundleArc, type BundleLayout, type BundlePoint } from "./bundle";
import type { AreaSegment, GraphModel, GraphNode } from "./model";
import { axialToPixel, buildTerritory, cellKey, NEIGHBOURS } from "./territory";

/** Hex's frame around the honeycomb: the skills' ring and the areas' markers inside it, the cells' outer edge. */
export const RING = {
  skills: 0.17,
  areas: 0.3,
  filesOuter: 0.84,
} as const;

/** Where the honeycomb's cells begin (graph units): clear of the skill and area markers in the middle. */
const HEX_FILES_INNER = 0.72;

function placeRing(nodes: GraphNode[], radius: number, startAngle = -Math.PI / 2): void {
  const n = nodes.length;
  nodes.forEach((node, i) => {
    const a = startAngle + (i / Math.max(n, 1)) * Math.PI * 2;
    node.x = Math.cos(a) * radius;
    node.y = Math.sin(a) * radius;
  });
}

/** The Second Brain's layouts (`core/brainView.ts` holds the list the settings offer). */
export type LayoutKind = BrainLayout;

/** Most icon nodes on the dashboard orbit ring (the inner of the two rings
 *  now that the App Ring sits outside it) — skills always get
 *  a slot each; recent files fill the rest up to this cap. Raised from the
 *  original `36` on owner feedback that too few recent files were showing. */
export const ORBIT_MAX = 60;

/** Dashboard-centre layout (the reference look): skills and the
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
  const ring = model.nodes.filter((n) => n.kind === "skill");
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

const TWO_PI = Math.PI * 2;

/**
 * The turn of the orbit ring (radians, added to every slot's angle) at which it rests when the motion is paused:
 * the newest note on the ring sits at 12 o'clock, the older ones follow clockwise (the ring is laid out newest
 * first after the skills). `0` when no note is on the ring. Skills keep their slots; only the whole ring turns.
 */
export function orbitRestAngle(model: GraphModel): number {
  let newest: GraphNode | null = null;
  let newestAt = -Infinity;
  for (const n of model.nodes) {
    if (!n.onOrbit || n.kind !== "file" || !n.modified) continue;
    const at = Date.parse(n.modified);
    if (Number.isNaN(at) || at <= newestAt) continue;
    newest = n;
    newestAt = at;
  }
  return newest ? -Math.PI / 2 - Math.atan2(newest.y, newest.x) : 0;
}

/** The signed way from angle `from` to `to` that is shortest, in (−π, π]. */
export function shortestTurn(from: number, to: number): number {
  const d = (((to - from) % TWO_PI) + TWO_PI) % TWO_PI;
  return d > Math.PI ? d - TWO_PI : d;
}

/** Angle left at which a turn towards a target counts as arrived (radians, well below one pixel on the ring). */
const ARRIVED = 1e-3;

/**
 * One frame of the ring settling into its rest angle: moves `from` towards `to` by the shortest way, covering the
 * share `1 − e^(−rate·dt)` of what is left (ease-out; `rate` 5 ≈ one second to settle), and lands on `to` exactly
 * once it is close enough to not be seen.
 */
export function approachAngle(from: number, to: number, dt: number, rate: number): number {
  const left = shortestTurn(from, to);
  if (Math.abs(left) < ARRIVED) return to;
  return from + left * (1 - Math.exp(-rate * dt));
}

/** App-Ring radius, in the same graph-unit scale as the orbit ring's radius
 *  1 (`layoutOrbit` above) — drawn just outside it. */
export const APP_RING = 1.18;

/** Angular step between successive app-ring icons, going outward from the
 *  "+" at 12 o'clock — fixed, not spread across the available arc, so a
 *  single icon sits snug next to the "+" instead of floating mid-quadrant.
 *  Deliberately uncapped for v1 (see `docs/plans/app-ring.md`) — somewhere
 *  around 12 apps per side the icons start to crowd. */
const APP_RING_STEP = 0.26;

/** Positions the App-Ring's builtin (left of the "+") and user-app (right of
 *  the "+") nodes, mutating `x`/`y` in place — the counterpart to
 *  `model.ts`'s `buildAppNodes`, which builds the node objects this expects
 *  to receive. Static: no `angle`/time dependency, unlike the inner orbit
 *  ring's `spin`. That alone isn't enough to keep the ring visually still,
 *  though — `render.ts` must also draw these nodes without adding its own
 *  live spin on top, or the "static" layout would still appear to rotate. */
export function layoutAppRing(builtins: GraphNode[], userApps: GraphNode[]): void {
  const plus = -Math.PI / 2;
  builtins.forEach((n, i) => {
    const a = plus - APP_RING_STEP * (i + 1);
    n.x = Math.cos(a) * APP_RING;
    n.y = Math.sin(a) * APP_RING;
  });
  userApps.forEach((n, i) => {
    const a = plus + APP_RING_STEP * (i + 1);
    n.x = Math.cos(a) * APP_RING;
    n.y = Math.sin(a) * APP_RING;
  });
}

/** Radius of a group's expanded secondary ring — further out than the App
 *  Ring itself, same graph-unit scale as `APP_RING`. Kept close to it
 *  (small delta): the App Ring's own radius is already tuned to clear the
 *  fixed header above the canvas (owner feedback on an earlier, larger
 *  delta — members near 12 o'clock scrolled up under the header bar), so
 *  any further push outward eats directly into that same margin. */
export const EXPANDED_GROUP_RING = APP_RING + 0.09;

/** Angular step between adjacent members on the expanded ring, centered on
 *  the group's own ring-slot angle (`anchorAngle`) — a per-member spacing
 *  rather than a fixed total arc, so a 2-member group sits as a tight,
 *  clearly-"grouped" pair instead of snapping straight to the two far ends
 *  of a wide arc (owner feedback: a 2-member group looked oddly spread
 *  out). The total span still grows with membership count, capped so a
 *  large group doesn't fan out indefinitely. */
const EXPANDED_GROUP_STEP = 0.07;
const EXPANDED_GROUP_MAX_ARC = 1.1;

/** Positions one group's member nodes (`onExpandedRing`, from
 *  `model.ts`'s `buildAppNodes`) on the expanded secondary ring, spread
 *  across `EXPANDED_GROUP_STEP` radians per member (capped at
 *  `EXPANDED_GROUP_MAX_ARC` total) centered on `anchorAngle` (the group's
 *  own already-laid-out ring-slot angle). A single member sits exactly at
 *  `anchorAngle` — a group of one is the normal starting state, not a
 *  degenerate case, so it gets no arbitrary offset. Static, like
 *  `layoutAppRing` — no time/angle dependency. */
export function layoutExpandedGroup(members: GraphNode[], anchorAngle: number): void {
  const n = members.length;
  const span = Math.min(EXPANDED_GROUP_MAX_ARC, EXPANDED_GROUP_STEP * Math.max(0, n - 1));
  members.forEach((m, i) => {
    const a = n > 1 ? anchorAngle - span / 2 + (i / (n - 1)) * span : anchorAngle;
    m.x = Math.cos(a) * EXPANDED_GROUP_RING;
    m.y = Math.sin(a) * EXPANDED_GROUP_RING;
  });
}

export function applyLayout(model: GraphModel, kind: LayoutKind): void {
  if (kind === "map") layoutMap(model);
  else if (kind === "hex") layoutHex(model);
  else layoutBundle(model);
}

/** Hex's frame: the hub at the centre, the skills on a small ring around it, each area's marker at the middle of its
 *  wedge — the honeycomb (`layoutHex`) fills the band outside them. */
function layoutHexFrame(model: GraphModel): void {
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
  for (const seg of model.areas) {
    const node = model.byId.get(`area:${seg.name}`);
    if (!node) continue;
    const a = (seg.start + seg.end) / 2;
    node.x = Math.cos(a) * RING.areas;
    node.y = Math.sin(a) * RING.areas;
  }
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

/** One candidate hex cell in the ring-spiral, in pixel (graph-unit) space —
 *  `ring`/`angle` are kept alongside `x`/`y` because `assignFilesToCells`
 *  picks cells innermost-first, angle-sorted within a ring. */
interface HexCell {
  x: number;
  y: number;
  ring: number;
  angle: number;
}

/** Solves one hex circumradius `u` (graph units) so `fileCount` hexes, each
 *  of area (3√3/2)u², cover roughly the band from `HEX_FILES_INNER` to
 *  `RING.filesOuter` (with a little slack so the outermost ring isn't
 *  razor-tight against neighbours) — plus the ring-spiral index `kStart` to
 *  start generating cells from. */
function solveHexUnit(fileCount: number): { u: number; kStart: number } {
  const innerR = HEX_FILES_INNER;
  const outerR = RING.filesOuter;
  const slack = 1.15;
  const fieldArea = Math.PI * (outerR * outerR - innerR * innerR);
  const hexArea = (3 * Math.sqrt(3)) / 2;
  const u = Math.sqrt((fieldArea * slack) / (fileCount * hexArea));
  const ringHeight = Math.sqrt(3) * u;
  const kStart = Math.max(1, Math.ceil(innerR / ringHeight));
  return { u, kStart };
}

/** Every hex cell from ring `kStart` outward, spiralling ring by ring
 *  (`hexRing`), until there are comfortably more candidates than
 *  `targetCount` files (30% slack) or the spiral has clearly run away —
 *  the slack absorbs `assignFilesToCells`'s per-wedge discretisation (an
 *  area's own cells can run out a little short even when the whole field
 *  has room to spare). */
function generateHexCells(unit: number, kStart: number, targetCount: number): HexCell[] {
  const cells: HexCell[] = [];
  let k = kStart;
  while (cells.length < targetCount * 1.3 && k < kStart + 200) {
    for (const [q, r] of hexRing(k)) {
      const [x, y] = hexToPixel(q, r, unit);
      cells.push({ x, y, ring: k, angle: Math.atan2(y, x) });
    }
    k++;
  }
  return cells;
}

/** Hands each area's files the nearest free cells inside its own angular
 *  wedge, newest first (innermost ring first, so the newest notes sit
 *  nearest the hub and an area darkens outward as it ages — owner, 2026-10-29; a note without a time goes last;
 *  ties by name, so the layout is stable), then spills any leftovers — a wedge that ran short
 *  because of the discretisation above, not a real area imbalance — onto
 *  whatever cells are still free, nearest first. Mutates each file node's
 *  `x`/`y` in place. */
function assignFilesToCells(files: GraphNode[], areas: AreaSegment[], cells: HexCell[]): void {
  // Loose (no-area) files have no wedge of their own: a narrow slice at 6 o'clock.
  const wedges = [
    ...areas.map((s) => ({ area: s.name, start: s.start, end: s.end })),
    { area: null, start: Math.PI / 2 - 0.15, end: Math.PI / 2 + 0.15 },
  ];
  const byArea = new Map<string | null, GraphNode[]>();
  for (const n of files) {
    const key = n.area;
    if (!byArea.has(key)) byArea.set(key, []);
    byArea.get(key)!.push(n);
  }

  const used = new Set<number>();
  const assigned = new Set<GraphNode>();
  for (const w of wedges) {
    const mine = (byArea.get(w.area) ?? [])
      .slice()
      .sort((a, b) => (bornOf(b) ?? -Infinity) - (bornOf(a) ?? -Infinity) || a.label.localeCompare(b.label));
    if (mine.length === 0) continue;
    const candidates = cells
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
    const free = cells
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
}

/** Honeycomb layout: hub, skills and area markers in the middle
 *  (`layoutHexFrame`); every file gets its own hex cell, tiled as a true
 *  flat-top hex grid (a spiral of rings out from the origin, per `hexRing`)
 *  so neighbouring notes touch edge-to-edge like a honeycomb. Cells are
 *  handed out per area, in the angular wedge each area owns
 *  (`model.areas`), so the overall shape still reads as "areas around a
 *  hub" — just made of hex tiles. One shared cell size (`model.hexUnit`,
 *  graph units) is solved from the file count so the mosaic fills the band,
 *  then everything is rescaled to land exactly on that band's outer edge;
 *  the renderer reads `hexUnit` back to draw each cell at the matching pixel
 *  size. */
export function layoutHex(model: GraphModel): void {
  layoutHexFrame(model);
  const files = model.nodes.filter((n) => n.kind === "file");
  if (files.length === 0) {
    model.hexUnit = undefined;
    return;
  }
  let { u, kStart } = solveHexUnit(files.length);
  const cells = generateHexCells(u, kStart, files.length);
  assignFilesToCells(files, model.areas, cells);

  // Rescale so the outermost placed cell lands on `RING.filesOuter` — fills
  // the same band regardless of how the ring-spiral vs. annulus area
  // estimates happened to compare, and keeps `hexUnit` in step with the
  // final layout.
  const maxR = Math.max(...files.map((n) => Math.hypot(n.x, n.y)), 1e-6);
  const scale = RING.filesOuter / maxR;
  u *= scale;
  for (const n of files) {
    n.x *= scale;
    n.y *= scale;
  }
  model.hexUnit = u;
}

/** How far out the map reaches, in graph units (1 = the lens circle's edge): room is left for the skills' ring. */
export const MAP_EDGE = 0.8;
/** The ring the skills sit on around the map — inside the lens's faded rim (`SecondBrainView`'s `LENS_FADE`, 0.9). */
export const MAP_SKILLS = 0.87;

/** A note's birth for the map and the hive (G17): its `created`, else its last change, else unknown. */
export function bornOf(n: GraphNode): number | null {
  const at = Date.parse(n.created ?? n.modified ?? "");
  return Number.isNaN(at) ? null : at;
}

/**
 * The territory map (UX5, G16): every note a field, every area a country (`graph/territory.ts`), centred and scaled
 * so the map fills the lens up to `MAP_EDGE`; `model.hexUnit` is a field's circumradius in graph units and
 * `model.territory` finds a field's neighbours. An area's node sits at its country's middle, where its name is drawn;
 * the skills ring the map; the hub has no place on it and stays at the centre, not drawn.
 */
export function layoutMap(model: GraphModel): void {
  const hub = model.byId.get("hub");
  if (hub) {
    hub.x = 0;
    hub.y = 0;
  }
  placeRing(
    model.nodes.filter((n) => n.kind === "skill"),
    MAP_SKILLS,
  );
  const files = model.nodes.filter((n) => n.kind === "file");
  const cells = buildTerritory(files.map((n) => ({ id: n.id, area: n.area, born: bornOf(n) })));
  const points = cells.map((c) => axialToPixel(c.q, c.r));
  const mx = points.reduce((s, [x]) => s + x, 0) / Math.max(1, points.length);
  const my = points.reduce((s, [, y]) => s + y, 0) / Math.max(1, points.length);
  // The farthest field's outer corner lands on the edge.
  let reach = 1;
  for (const [x, y] of points) reach = Math.max(reach, Math.hypot(x - mx, y - my) + 1);
  const unit = MAP_EDGE / reach;
  const territory = new Map<string, GraphNode>();
  cells.forEach((c, i) => {
    const node = model.byId.get(c.id);
    const [x, y] = points[i] ?? [0, 0];
    if (!node) return;
    node.x = (x - mx) * unit;
    node.y = (y - my) * unit;
    node.cell = { q: c.q, r: c.r };
    territory.set(cellKey(c.q, c.r), node);
  });
  model.hexUnit = cells.length > 0 ? unit : undefined;
  model.territory = territory;
  model.mapCentre = { x: mx, y: my };
  // Each field's six neighbours, found once here rather than by key in every frame (the borders).
  for (const node of territory.values()) {
    const at = node.cell;
    if (at) node.around = NEIGHBOURS.map(({ dq, dr }) => territory.get(cellKey(at.q + dq, at.r + dr)) ?? null);
  }

  for (const area of model.nodes.filter((n) => n.kind === "area")) {
    const mine = files.filter((n) => n.area === area.area);
    area.x = mine.reduce((s, n) => s + n.x, 0) / Math.max(1, mine.length);
    area.y = mine.reduce((s, n) => s + n.y, 0) / Math.max(1, mine.length);
  }
}

/** The Bundle layout's radii (graph units): the notes' ring, the areas' coloured band just outside it, their icons and
 *  names further out, and the skills' ring outside all of them — inside the lens's faded rim (0.9). */
export const BUNDLE = {
  notes: 0.6,
  band: 0.625,
  areas: 0.665,
  names: 0.715,
  skills: 0.82,
} as const;

/** The most angle between two arcs (radians); fewer is taken when many arcs share the ring (folders). */
const BUNDLE_GAP = 0.05;
/** The share of the ring the gaps may take at most, however many arcs there are. */
const BUNDLE_GAP_SHARE = 0.2;

/** A folder path's segments, so that siblings sort together ("Foo/Bar" right after "Foo", not after "Foo Baz"). */
function compareFolders(a: string, b: string): number {
  const as = a.split("/");
  const bs = b.split("/");
  for (let i = 0; i < Math.min(as.length, bs.length); i++) {
    const c = as[i]!.localeCompare(bs[i]!);
    if (c !== 0) return c;
  }
  return as.length - bs.length;
}

/** Newest first; a note without a time last; ties by name, so the ring is stable. */
function newestFirst(a: GraphNode, b: GraphNode): number {
  return (bornOf(b) ?? -Infinity) - (bornOf(a) ?? -Infinity) || a.label.localeCompare(b.label);
}

/**
 * The Bundle layout (`docs/plans/brain-views.md`, BV1): every note on one ring, an arc per area — per deepest folder
 * when grouped by folders, which makes the ring finer (Q8) — with a gap between arcs; on each arc the newest note
 * first, the older ones clockwise after it (Q4). Area icons sit outside the ring, the skills on a second ring further
 * out (Q5); nothing in the middle, where the links run. `model.bundle` keeps the tree the links bundle along: each
 * note's parent is its area (or folder, then that folder's parents), the area's the root — every inner node at the
 * middle of its notes' angles, the further from the ring the more levels it has below it (the radii of d3's cluster
 * layout).
 */
export function layoutBundle(model: GraphModel): void {
  const hub = model.byId.get("hub");
  if (hub) {
    hub.x = 0;
    hub.y = 0;
  }
  placeRing(
    model.nodes.filter((n) => n.kind === "skill"),
    BUNDLE.skills,
  );

  const files = model.nodes.filter((n) => n.kind === "file");
  const byArea = new Map<string | null, GraphNode[]>();
  for (const n of files) {
    const list = byArea.get(n.area);
    if (list) list.push(n);
    else byArea.set(n.area, [n]);
  }
  const named = [...byArea.keys()].filter((a): a is string => a !== null).sort(compareFolders);
  const order: (string | null)[] = byArea.has(null) ? [...named, null] : named;

  const gap = order.length > 1 ? Math.min(BUNDLE_GAP, (Math.PI * 2 * BUNDLE_GAP_SHARE) / order.length) : 0;
  const usable = Math.PI * 2 - gap * order.length;
  const total = Math.max(1, files.length);
  let angle = -Math.PI / 2 + gap / 2;
  const arcs: BundleArc[] = [];
  const angleOf = new Map<string, number>();
  for (const area of order) {
    const mine = (byArea.get(area) ?? []).slice().sort(newestFirst);
    const span = (mine.length / total) * usable;
    arcs.push({ area, start: angle, end: angle + span, count: mine.length });
    mine.forEach((n, i) => {
      const a = angle + ((i + 0.5) / mine.length) * span;
      angleOf.set(n.id, a);
      n.x = Math.cos(a) * BUNDLE.notes;
      n.y = Math.sin(a) * BUNDLE.notes;
    });
    if (area !== null) {
      const node = model.byId.get(`area:${area}`);
      if (node) {
        const mid = angle + span / 2;
        node.x = Math.cos(mid) * BUNDLE.areas;
        node.y = Math.sin(mid) * BUNDLE.areas;
      }
    }
    angle += span + gap;
  }

  // The tree: root → each folder level of the area's name → the notes.
  const parent = new Map<string, string>();
  const children = new Map<string, string[]>();
  const link = (child: string, up: string) => {
    if (parent.has(child)) return;
    parent.set(child, up);
    children.set(up, [...(children.get(up) ?? []), child]);
  };
  for (const arc of arcs) {
    let up = BUNDLE_ROOT;
    if (arc.area !== null) {
      const parts = arc.area.split("/");
      for (let i = 1; i <= parts.length; i++) {
        const key = dirKey(parts.slice(0, i).join("/"));
        link(key, up);
        up = key;
      }
    }
    for (const n of byArea.get(arc.area) ?? []) link(n.id, up);
  }
  // A child's order follows the ring: its own notes and folders in the order they were linked, which is the arcs'.
  for (const list of children.values()) list.sort((a, b) => (firstAngle(a) ?? 0) - (firstAngle(b) ?? 0));
  function firstAngle(key: string): number | undefined {
    const own = angleOf.get(key);
    if (own !== undefined) return own;
    const kids = children.get(key) ?? [];
    return kids.length > 0 ? firstAngle(kids[0]!) : undefined;
  }

  const height = new Map<string, number>();
  const heightOf = (key: string): number => {
    const known = height.get(key);
    if (known !== undefined) return known;
    const kids = children.get(key) ?? [];
    const h = kids.length === 0 ? 0 : 1 + Math.max(...kids.map(heightOf));
    height.set(key, h);
    return h;
  };
  const rootHeight = Math.max(1, heightOf(BUNDLE_ROOT));
  const points = new Map<string, BundlePoint>();
  const angles = new Map<string, number>();
  const place = (key: string): number => {
    const done = angles.get(key);
    if (done !== undefined) return done;
    const kids = children.get(key) ?? [];
    const kidAngles = kids.map(place);
    // d3's cluster: an inner node lies midway between its first and its last child.
    const a = angleOf.get(key) ?? (kids.length > 0 ? (kidAngles[0]! + kidAngles[kids.length - 1]!) / 2 : -Math.PI / 2);
    angles.set(key, a);
    const r = key === BUNDLE_ROOT ? 0 : BUNDLE.notes * (1 - heightOf(key) / rootHeight);
    points.set(key, { x: Math.cos(a) * r, y: Math.sin(a) * r });
    return a;
  };
  place(BUNDLE_ROOT);
  model.bundle = { arcs, points, parent } satisfies BundleLayout;
  model.hexUnit = undefined;
}
