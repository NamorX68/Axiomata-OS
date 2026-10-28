/**
 * The Orbit's honeycomb sphere (`docs/plans/ui-ux-round.md` UX4, W1/W2, G7): the vault as a ball of hex cells. Every
 * note is a cell in its area's colour (the one its icon has on the Orbit's ring), every area a coherent landmass, and
 * there are more cells than notes — the rest stay dark, "unsettled", so the ball is whole however few notes there are.
 * A cell carries an ember where something was touched today and darkens, keeping its hue, where nothing moved for weeks
 * (ember ageing); under the "metal" shimmer each cell is lit by its real
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

/** A note as the hive needs it: its area (`null` = loose) and its birth (ms), `null` or absent when unknown. */
export interface HiveFile {
  id: string;
  area: string | null;
  born?: number | null;
}

/** How many cells there are per note: the rest stay unsettled. */
export const SPARE_CELLS = 1.3;
/** The fewest cells a sphere has, so a young vault is still a ball. */
export const MIN_CELLS = 90;
/** How many nearest cells count as a cell's neighbours: the lattice is a near-hexagonal tiling. */
const NEIGHBOUR_COUNT = 6;

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

/**
 * Each point's neighbours on a Fibonacci lattice (`fibonacciSphere`): its `k` nearest points, made mutual, as sorted
 * indices. The lattice runs from pole to pole with `y` falling evenly by index, so a point's neighbours lie within a
 * window of about 4·√n indices — searched there instead of over the whole ball.
 */
export function sphereNeighbours(points: readonly Vec3[], k = NEIGHBOUR_COUNT): number[][] {
  const n = points.length;
  const window = Math.ceil(4 * Math.sqrt(n)) + k;
  const sets = points.map(() => new Set<number>());
  points.forEach((p, i) => {
    const near: { j: number; d: number }[] = [];
    for (let j = Math.max(0, i - window); j <= Math.min(n - 1, i + window); j++) {
      if (j !== i) near.push({ j, d: dist2(p, points[j] as Vec3) });
    }
    near.sort((a, b) => a.d - b.d || a.j - b.j);
    for (const { j } of near.slice(0, k)) {
      sets[i]?.add(j);
      sets[j]?.add(i);
    }
  });
  return sets.map((set) => [...set].sort((a, b) => a - b));
}

/** A stable 0…1 number from a string, for phases that survive a rebuild. */
function hash01(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return ((h >>> 0) % 10000) / 10000;
}

/** One area growing over the ball (`buildHive`). */
interface Region {
  ids: string[];
  seed: Vec3;
  claimed: number;
  /** Free cells next to the region, by index. */
  frontier: Set<number>;
}

/**
 * The sphere for a vault: one region of adjacent cells per area, so an area reads as one landmass in its colour (the
 * colour its notes have on the Orbit's ring). The seeds are spread evenly over the ball, the largest area first; the
 * regions grow a cell at a time over the lattice's neighbours — always the region furthest behind its own size, always
 * its free neighbour nearest its seed — so they stay round and finish together, the oldest notes at the heart and the
 * youngest on the coast. A region walled in by its neighbours goes on in the free cell nearest its seed (an exclave)
 * rather than stop. The spare cells stay as unsettled seams between the regions. Deterministic: the same files give the
 * same ball.
 */
export function buildHive(files: readonly HiveFile[]): HiveCell[] {
  const total = Math.max(MIN_CELLS, Math.ceil(files.length * SPARE_CELLS));
  const points = fibonacciSphere(total);
  const neighbours = sphereNeighbours(points);
  const byArea = new Map<string, HiveFile[]>();
  for (const f of files) {
    const key = f.area ?? "";
    const list = byArea.get(key) ?? [];
    list.push(f);
    byArea.set(key, list);
  }
  const byBirth = (a: HiveFile, b: HiveFile): number =>
    (a.born ?? Infinity) - (b.born ?? Infinity) || a.id.localeCompare(b.id);
  const areas = [...byArea.entries()]
    .map(([name, list]) => ({ name, ids: [...list].sort(byBirth).map((f) => f.id) }))
    .sort((a, b) => b.ids.length - a.ids.length || a.name.localeCompare(b.name));
  const seeds = fibonacciSphere(Math.max(1, areas.length));

  const owner: (string | null)[] = points.map(() => null);
  const nearestFree = (seed: Vec3): number => {
    let best = -1;
    let bestD = Infinity;
    owner.forEach((id, i) => {
      if (id !== null) return;
      const d = dist2(points[i] as Vec3, seed);
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    });
    return best;
  };
  const regions: Region[] = areas.map((area, i) => ({
    ids: area.ids,
    seed: seeds[i] ?? [0, 1, 0],
    claimed: 0,
    frontier: new Set<number>(),
  }));
  const claim = (region: Region, at: number): void => {
    owner[at] = region.ids[region.claimed] ?? null;
    region.claimed++;
    for (const j of neighbours[at] ?? []) if (owner[j] === null) region.frontier.add(j);
  };

  for (;;) {
    // The region furthest behind its own size goes next; ties go to the larger one (its index is lower).
    let region: Region | undefined;
    let lowest = Infinity;
    for (const r of regions) {
      if (r.claimed >= r.ids.length) continue;
      const progress = r.claimed / r.ids.length;
      if (progress < lowest) {
        lowest = progress;
        region = r;
      }
    }
    if (!region) break;
    let best = -1;
    let bestD = Infinity;
    for (const j of region.frontier) {
      if (owner[j] !== null) {
        region.frontier.delete(j);
        continue;
      }
      const d = dist2(points[j] as Vec3, region.seed);
      if (d < bestD || (d === bestD && j < best)) {
        bestD = d;
        best = j;
      }
    }
    const at = best >= 0 ? best : nearestFree(region.seed);
    if (at < 0) break;
    region.frontier.delete(at);
    claim(region, at);
  }
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

/** The share of its brightness the oldest note's cell gives up: it stays its area's colour, only darker. */
export const AGE_DARKENING = 0.62;

/**
 * A note's colour as it has aged: its area's colour `base` (the one its ring icon has), darkened by `ash` (0…1, from
 * `emberOf`) toward black — every channel scaled alike, so the hue stays exactly the area's. A fresh note shows its
 * heat as an ember drawn on top, not in this colour, or it would read as another area.
 */
export function agedRgb(base: Rgb, ash: number): Rgb {
  const k = 1 - AGE_DARKENING * Math.min(1, Math.max(0, ash));
  return [base[0] * k, base[1] * k, base[2] * k];
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

/**
 * The cell under a point: the index of the nearest pickable cell (`pickable[i]` 1) whose centre (`xs[i]`, `ys[i]`) lies
 * within `reach` px of (`px`, `py`), or -1. The cells tile the ball, so the nearest centre is the cell the point is on.
 */
export function nearestCell(
  xs: ArrayLike<number>,
  ys: ArrayLike<number>,
  pickable: ArrayLike<number>,
  px: number,
  py: number,
  reach: number,
): number {
  let best = -1;
  let bestD = reach * reach;
  for (let i = 0; i < pickable.length; i++) {
    if (!pickable[i]) continue;
    const d = ((xs[i] ?? 0) - px) ** 2 + ((ys[i] ?? 0) - py) ** 2;
    if (d < bestD) {
      bestD = d;
      best = i;
    }
  }
  return best;
}

/** How far the owner may tip the hive towards a pole (radians), so it never turns over. */
export const MAX_TILT = 1.1;

/**
 * A drag of (`dx`, `dy`) px over a ball of radius `r` px as a turn: the yaw it adds (sideways, about the axis) and the
 * new tilt (up and down, bounded by `MAX_TILT`), so the surface under the pointer follows it.
 */
export function dragTurn(tilt: number, dx: number, dy: number, r: number): { yaw: number; tilt: number } {
  const radius = Math.max(1, r);
  return { yaw: -dx / radius, tilt: Math.min(MAX_TILT, Math.max(-MAX_TILT, tilt - dy / radius)) };
}

/** The fastest a let-go hive coasts on (radians per second): a flick turns it briskly, never a blur. */
export const MAX_COAST = 6;
/** A drag held still this long (ms) before it is let go ends without coasting. */
export const STILL_MS = 80;

/**
 * The speed a let-go hive coasts on: the drag's last speed (`spin`, radians per second), bounded by `MAX_COAST` — or
 * none, when the pointer stood still for `STILL_MS` before the release or the motion is not Full.
 */
export function coastSpin(spin: number, stillMs: number, full: boolean): number {
  if (!full || stillMs > STILL_MS) return 0;
  return Math.min(MAX_COAST, Math.max(-MAX_COAST, spin));
}
