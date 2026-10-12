/**
 * Ring layout (the reference look): hub at the centre, skills on an inner
 * ring, files on concentric arcs inside their area's angular segment.
 * Routines are not drawn (owner, 2026-10-07: they live on the Routines board). Positions are in graph units where 1 = the
 * outer radius; the renderer scales to the canvas.
 */

import type { GraphModel, GraphNode } from "./model";
import { axialToPixel, buildTerritory, cellKey, NEIGHBOURS } from "./territory";

export const RING = {
  skills: 0.17,
  areas: 0.3,
  filesInner: 0.39,
  filesOuter: 0.84,
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

export type LayoutKind = "rings" | "map";

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
  else layoutRings(model);
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
    // spacing?
    const rows: GraphNode[][] = [];
    let radius: number = RING.filesInner;
    let i = 0;
    while (i < mine.length) {
      const perArc = Math.max(1, Math.floor((span * radius) / ARC_STEP));
      rows.push(mine.slice(i, i + perArc));
      i += perArc;
      radius = Math.min(RING.filesOuter, radius + RING_STEP);
    }
    const bandSpan = RING.filesOuter - RING.filesInner;
    // Rows are spread a little past the natural minimum spacing so a vault
    // needing just a couple of rows doesn't look huddled at the very inner
    // edge — but capped at a modest multiple of RING_STEP. Spreading them
    // across the *whole* band instead (bandSpan / (rows.length - 1)) reads
    // fine with many rows (naturally close to that already), but for
    // exactly two rows it flung the second one all the way to the outer
    // edge, reading as an unrelated second cluster rather than more of the
    // same folder's files (owner feedback, live testing).
    const stepR = rows.length > 1 ? Math.min(RING_STEP * 3, Math.max(RING_STEP, bandSpan / (rows.length - 1))) : 0;
    rows.forEach((row, k) => {
      const r = Math.min(RING.filesOuter, RING.filesInner + k * stepR);
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

/** How far out the map reaches, in graph units (1 = the lens circle's edge): room is left for the skills' ring. */
export const MAP_EDGE = 0.8;
/** The ring the skills sit on around the map — inside the lens's faded rim (`SecondBrainView`'s `LENS_FADE`, 0.9). */
export const MAP_SKILLS = 0.87;

/** A note's birth for the map (G17): its `created`, else its last change, else unknown. */
function bornOf(n: GraphNode): number | null {
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
