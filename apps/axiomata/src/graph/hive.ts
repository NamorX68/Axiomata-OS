/**
 * The Orbit's honeycomb sphere (`docs/plans/ui-ux-round.md` UX4, W1/W2, G7): the vault as a ball of hex cells. Every
 * note is a cell, every area a coherent landmass, and there are more cells than notes — the rest stay dark,
 * "unsettled", so the ball is whole however few notes there are. The cells glow where something was touched today and
 * cool to ash where nothing moved for weeks (ember ageing); under the "metal" shimmer each cell is lit by its real
 * tilt on the sphere, with a highlight. Plain geometry and colour arithmetic here; `graph/render.ts` draws.
 */

export type Vec3 = [number, number, number];
export type Rgb = [number, number, number];

/** One cell of the sphere; `fileId` is `null` for an unsettled one. */
export interface HiveCell {
  /** Unit vector: where on the sphere the cell sits. */
  p: Vec3;
  fileId: string | null;
  /** A twinkle phase of its own, 0…1, so the cells do not breathe in step. */
  phase: number;
}

/** A note as the hive needs it. */
export interface HiveFile {
  id: string;
  area: string | null;
}

/** How many cells there are per note: the rest stay unsettled. */
export const SPARE_CELLS = 1.3;
/** The fewest cells a sphere has, so a young vault is still a ball. */
export const MIN_CELLS = 90;

const GOLDEN = Math.PI * (3 - Math.sqrt(5));

/** `n` points spread evenly over the unit sphere (the Fibonacci lattice). */
export function fibonacciSphere(n: number): Vec3[] {
  const points: Vec3[] = [];
  for (let i = 0; i < n; i++) {
    const y = 1 - (2 * (i + 0.5)) / n;
    const r = Math.sqrt(Math.max(0, 1 - y * y));
    const t = GOLDEN * i;
    points.push([Math.cos(t) * r, y, Math.sin(t) * r]);
  }
  return points;
}

const dist2 = (a: Vec3, b: Vec3): number => (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;

/** A stable 0…1 number from a string, for phases that survive a rebuild. */
function hash01(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return ((h >>> 0) % 10000) / 10000;
}

/**
 * The sphere for a vault: each area claims the free cells nearest its own seed — the largest area first — so areas
 * grow as landmasses; seeds are spread evenly over the sphere. Deterministic: the same files give the same ball.
 */
export function buildHive(files: readonly HiveFile[]): HiveCell[] {
  const total = Math.max(MIN_CELLS, Math.ceil(files.length * SPARE_CELLS));
  const points = fibonacciSphere(total);
  const byArea = new Map<string, string[]>();
  for (const f of files) {
    const key = f.area ?? "";
    const list = byArea.get(key) ?? [];
    list.push(f.id);
    byArea.set(key, list);
  }
  const areas = [...byArea.entries()]
    .map(([name, ids]) => ({ name, ids: [...ids].sort() }))
    .sort((a, b) => b.ids.length - a.ids.length || a.name.localeCompare(b.name));
  const seeds = fibonacciSphere(Math.max(1, areas.length));

  const owner: (string | null)[] = points.map(() => null);
  const free = new Set(points.keys());
  areas.forEach((area, i) => {
    const seed = seeds[i] ?? [0, 1, 0];
    const nearest = [...free].sort((p, q) => dist2(points[p] as Vec3, seed) - dist2(points[q] as Vec3, seed));
    area.ids.forEach((id, k) => {
      const at = nearest[k];
      if (at === undefined) return;
      owner[at] = id;
      free.delete(at);
    });
  });
  return points.map((p, i) => {
    const fileId = owner[i] ?? null;
    return { p, fileId, phase: hash01(fileId ?? `cell-${i}`) };
  });
}

/** How a cell's note has aged: `heat` 0…1 (touched within the last day, fading), `ash` 0…1 (still for weeks). */
export interface Ember {
  heat: number;
  ash: number;
}

/** A day in ms. */
const DAY = 86_400_000;
/** After how many days a note has cooled all the way to ash. */
export const ASH_DAYS = 40;

/** The ember of a note last changed at `modified` (ms), seen at `now`. A note without a time is half cooled. */
export function emberOf(modified: number | null, now: number): Ember {
  if (modified === null) return { heat: 0, ash: 0.5 };
  const days = Math.max(0, (now - modified) / DAY);
  return { heat: days < 1 ? 0.85 * (1 - days) : 0, ash: Math.min(1, days / ASH_DAYS) * 0.85 };
}

/**
 * The span the time travel covers (G17): from the earliest note's birth to `now`. A vault without dates spans a day,
 * so the slider still has somewhere to go.
 */
export function travelSpan(created: readonly (string | null | undefined)[], now: number): { from: number; to: number } {
  let from = Infinity;
  for (const text of created) {
    const at = text ? Date.parse(text) : NaN;
    if (!Number.isNaN(at) && at < from) from = at;
  }
  return { from: Number.isFinite(from) && from < now ? from : now - DAY, to: now };
}

/** `a` moved toward `b` by `t` (0…1). */
export function mixRgb(a: Rgb, b: Rgb, t: number): Rgb {
  const k = Math.min(1, Math.max(0, t));
  return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
}

/** The light falls from the upper left, towards the viewer (screen space: y down, z towards the eye) — normalised. */
const LIGHT: Vec3 = ((): Vec3 => {
  const l: Vec3 = [-0.45, -0.55, 0.7];
  const n = Math.hypot(...l);
  return [l[0] / n, l[1] / n, l[2] / n];
})();

/** The half-way vector between the light and the eye (Blinn–Phong), normalised: where the highlight sits. */
const HALF: Vec3 = ((): Vec3 => {
  const h: Vec3 = [LIGHT[0], LIGHT[1], LIGHT[2] + 1];
  const n = Math.hypot(...h);
  return [h[0] / n, h[1] / n, h[2] / n];
})();

/**
 * The "metal" shimmer for a cell whose surface normal is (`nx`, `ny`, `nz`) in screen space: how lit it is (diffuse,
 * 0.45…1.2) and how much highlight it catches (0…1). `sparkle` (0…1) lets the highlight flicker per cell like a
 * tilting plate; a still light passes 1. Called per cell and frame, so it allocates nothing but its answer.
 */
export function metalLight(nx: number, ny: number, nz: number, sparkle: number): { shade: number; spec: number } {
  const diffuse = Math.max(0, nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2]);
  const spec = Math.max(0, nx * HALF[0] + ny * HALF[1] + nz * HALF[2]) ** 40 * sparkle;
  return { shade: 0.45 + 0.75 * diffuse, spec };
}
