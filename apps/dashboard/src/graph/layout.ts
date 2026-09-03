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

export type LayoutKind = "rings" | "circle";

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
  if (kind === "circle") layoutCircle(model);
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
