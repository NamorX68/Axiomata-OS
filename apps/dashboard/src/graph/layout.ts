/**
 * Ring layout (the reference look): hub at the centre, skills on an inner
 * ring, files on concentric arcs inside their area's angular segment,
 * routines on the outer ring. Positions are in graph units where 1 = the
 * outer radius; the renderer scales to the canvas.
 */

import type { GraphModel, GraphNode } from "./model";

export const RING = {
  skills: 0.2,
  filesInner: 0.36,
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

export function layoutRings(model: GraphModel): void {
  const hub = model.byId.get("hub");
  if (hub) {
    hub.x = 0;
    hub.y = 0;
  }
  placeRing(
    model.nodes.filter((n) => n.kind === "skill"),
    RING.skills,
  );
  placeRing(
    model.nodes.filter((n) => n.kind === "routine"),
    RING.routines,
    -Math.PI / 2 + 0.15,
  );

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
