/**
 * "How your vault grew" (`docs/plans/brain-views.md`, BV2): a replay of the vault's life in a fixed time, whatever its
 * age (Q9). This holds the pure part — when the replay shows which moment, how far a note has sprouted, how long a
 * new link flashes, and where everything sits: a core in the middle, a branch out to each area, the area's notes as
 * hex cells on a sunflower spiral around the branch's end, the oldest at its heart. The renderer draws it.
 * Positions are in graph units (1 = the lens's edge).
 */

/** How long the replay takes from the first note to today (ms), whatever the vault's age (Q9). */
export const GROWTH_MS = 20_000;
/** How long the replay takes to fade into the chosen layout at its end (Q10), or when it is ended early. */
export const GROWTH_FADE_MS = 1_200;

/** How long a note takes to sprout to its full size, as a share of the replay (a quarter of a second at 20 s). */
const SPROUT_SHARE = 0.012;
/** How long a new note or link glows, as a share of the replay. */
const FLASH_SHARE = 0.04;
/** How long a branch takes to grow out to its area, as a share of the replay. */
const BRANCH_SHARE = 0.05;
/** The size a note starts at when it appears, as a share of its full size: seen at once, then growing. */
const SPROUT_START = 0.25;

/** Where the areas' spirals sit around the core, and how far the drawing may reach. */
const BRANCH_REACH = 0.5;
const EDGE = 0.84;
/** The room kept free between two neighbouring spirals, as a share of their distance. */
const SPIRAL_ROOM = 0.9;
/** A cell's circumradius as a share of the spiral's spacing: the cells nearly touch. */
const CELL_SHARE = 0.7;
/** The sunflower's turn from one note to the next. */
export const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

/** The vault's life, its first note's birth to now (`graph/hive.ts` `travelSpan`). */
export interface GrowthSpan {
  from: number;
  to: number;
}

/** A note as the replay needs it. */
export interface GrowthFile {
  id: string;
  area: string | null;
  born: number | null;
}

/** One area's branch: where its spiral sits, when its first note came, and how many notes it holds in the end. */
export interface GrowthBranch {
  area: string | null;
  x: number;
  y: number;
  /** The birth of the area's first note; `null` when none of its notes has a time (there from the start). */
  first: number | null;
  count: number;
}

/** A note's place: its cell's middle, its rank on its area's spiral (0 = the heart) and its birth. */
export interface GrowthCell {
  x: number;
  y: number;
  rank: number;
  area: string | null;
  born: number | null;
}

export interface GrowthLayout {
  branches: GrowthBranch[];
  cells: Map<string, GrowthCell>;
  /** The sunflower's spacing (graph units): the `k`-th note lies `spacing·√(k + ½)` from its spiral's middle. */
  spacing: number;
  /** A cell's circumradius (graph units). */
  unit: number;
}

const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

/** The moment the replay shows after `elapsed` ms: the vault's life mapped onto {@link GROWTH_MS}, linearly. */
export function growthMoment(elapsed: number, span: GrowthSpan): number {
  return span.from + (span.to - span.from) * clamp01(elapsed / GROWTH_MS);
}

/** The share `share` of the replay, as vault time (ms). */
const shareOf = (span: GrowthSpan, share: number): number => Math.max(1, (span.to - span.from) * share);

/** How far a note born at `born` has sprouted at the moment `at`: 0 before it is born, then from
 *  {@link SPROUT_START} up to 1 within a moment of the replay. A note without a time is there from the start. */
export function sprout(at: number, born: number | null, span: GrowthSpan): number {
  if (born === null) return 1;
  if (at < born) return 0;
  return SPROUT_START + (1 - SPROUT_START) * clamp01((at - born) / shareOf(span, SPROUT_SHARE));
}

/** How brightly something born at `born` still flashes at `at`: 1 the moment it comes, 0 a little later and before. */
export function flash(at: number, born: number | null, span: GrowthSpan): number {
  if (born === null || at < born) return 0;
  return 1 - clamp01((at - born) / shareOf(span, FLASH_SHARE));
}

/** How far an area's branch reaches from the core at `at` (0…1): it grows out when the area's first note comes. */
export function branchGrowth(at: number, first: number | null, span: GrowthSpan): number {
  if (first === null) return 1;
  if (at < first) return 0;
  return clamp01((at - first) / shareOf(span, BRANCH_SHARE));
}

/** How old a note looks at `at` (0…1): the replay darkens a note as it ages, as the layouts do. */
export function ashAt(at: number, born: number | null, span: GrowthSpan): number {
  if (born === null) return clamp01((at - span.from) / shareOf(span, 0.35)) * 0.85;
  return clamp01((at - born) / shareOf(span, 0.35)) * 0.85;
}

/** The `k`-th point of a sunflower spiral with spacing `spacing`, around the origin. */
export function sunflower(k: number, spacing: number): { x: number; y: number } {
  const r = spacing * Math.sqrt(k + 0.5);
  const a = k * GOLDEN_ANGLE;
  return { x: Math.cos(a) * r, y: Math.sin(a) * r };
}

/** How many of the births in `sorted` (ascending) lie at or before `at`. */
export function countBornBy(sorted: readonly number[], at: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (sorted[mid]! <= at) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Oldest first; a note without a time counts as the oldest (it is there from the start); ties by id. */
function oldestFirst(a: GrowthFile, b: GrowthFile): number {
  return (a.born ?? -Infinity) - (b.born ?? -Infinity) || a.id.localeCompare(b.id);
}

/**
 * Where the replay puts everything. The areas' spirals sit in `order` (the areas without a note left out, the
 * notes at the vault's root as an area of their own at the end) around the core, each in an angle as wide as its
 * spiral; the spacing is the largest that keeps neighbouring spirals apart and every spiral inside the lens. A single
 * area grows from the core itself.
 */
export function layoutGrowth(files: readonly GrowthFile[], order: readonly (string | null)[]): GrowthLayout {
  const byArea = new Map<string | null, GrowthFile[]>();
  for (const f of files) {
    const list = byArea.get(f.area);
    if (list) list.push(f);
    else byArea.set(f.area, [f]);
  }
  const known = new Set(order);
  const areas = [...order.filter((a) => byArea.has(a)), ...[...byArea.keys()].filter((a) => !known.has(a))];
  const k = areas.length;
  const reach = k > 1 ? BRANCH_REACH : 0;
  // Each area takes an angle as wide as its spiral (√ of its notes), so a large one has room beside a small one.
  const sizes = areas.map((a) => Math.sqrt((byArea.get(a)?.length ?? 0) + 0.5));
  const total = sizes.reduce((sum, size) => sum + size, 0);
  const mids: number[] = [];
  let turn = -Math.PI / 2;
  for (const size of sizes) {
    mids.push(turn + (size / total) * Math.PI);
    turn += (size / total) * Math.PI * 2;
  }
  // The first area's middle at 12 o'clock.
  const shift = -Math.PI / 2 - (mids[0] ?? -Math.PI / 2);
  const centres = mids.map((mid) =>
    reach === 0 ? { x: 0, y: 0 } : { x: Math.cos(mid + shift) * reach, y: Math.sin(mid + shift) * reach },
  );
  let spacing = Infinity;
  sizes.forEach((size, i) => {
    spacing = Math.min(spacing, (EDGE - reach) / size);
    if (k > 1) {
      const j = (i + 1) % k;
      const apart = Math.hypot(centres[i]!.x - centres[j]!.x, centres[i]!.y - centres[j]!.y);
      spacing = Math.min(spacing, (apart * SPIRAL_ROOM) / (size + sizes[j]!));
    }
  });
  if (!Number.isFinite(spacing) || spacing <= 0) spacing = 0.05;

  const branches: GrowthBranch[] = [];
  const cells = new Map<string, GrowthCell>();
  areas.forEach((area, i) => {
    const mine = (byArea.get(area) ?? []).slice().sort(oldestFirst);
    const centre = centres[i]!;
    const first = mine.find((f) => f.born !== null)?.born ?? null;
    branches.push({ area, x: centre.x, y: centre.y, first: mine[0]?.born === null ? null : first, count: mine.length });
    mine.forEach((f, rank) => {
      const p = sunflower(rank, spacing);
      cells.set(f.id, { x: centre.x + p.x, y: centre.y + p.y, rank, area, born: f.born });
    });
  });
  return { branches, cells, spacing, unit: spacing * CELL_SHARE };
}
