/**
 * A honeycomb of hexagons around a centre cell (pointy-top, like the logo), for the empty canvas's growing comb
 * (`canvas/EmptyCanvas.svelte`, UX3). Plain geometry in axial coordinates.
 */

export interface Cell {
  /** Centre, in units of the cell's circumradius. */
  x: number;
  y: number;
  /** How many steps from the centre cell: the order the comb grows in. */
  ring: number;
}

/** Every cell up to `rings` steps from the centre (1 + 3·rings·(rings+1) cells), the centre first, ring by ring. */
export function honeycomb(rings: number): Cell[] {
  const cells: Cell[] = [];
  for (let q = -rings; q <= rings; q++) {
    for (let r = Math.max(-rings, -q - rings); r <= Math.min(rings, -q + rings); r++) {
      const ring = Math.max(Math.abs(q), Math.abs(r), Math.abs(-q - r));
      cells.push({ x: Math.sqrt(3) * (q + r / 2), y: 1.5 * r, ring });
    }
  }
  return cells.sort((a, b) => a.ring - b.ring || Math.atan2(a.y, a.x) - Math.atan2(b.y, b.x));
}

/** The six corners of a pointy-top hexagon around (`cx`, `cy`), as SVG points. */
export function hexPoints(cx: number, cy: number, radius: number): string {
  return Array.from({ length: 6 }, (_, i) => {
    const angle = (Math.PI / 3) * i - Math.PI / 2;
    return `${(cx + radius * Math.cos(angle)).toFixed(2)},${(cy + radius * Math.sin(angle)).toFixed(2)}`;
  }).join(" ");
}
