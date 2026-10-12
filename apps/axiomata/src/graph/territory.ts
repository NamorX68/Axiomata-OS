/**
 * The Second Brain's territory map (`docs/plans/ui-ux-round.md` UX5, G16): the vault as a map of hex fields. Every note
 * is a field, every area (or folder) one coherent country grown from its own seed. A country grows from its heart
 * outward in the order its notes were born, so the oldest notes sit in the middle, the youngest on the coast — and the
 * time travel lets the countries grow. Plain geometry on an axial pointy-top grid (unit circumradius); `layout.ts`
 * scales it into graph units, `render.ts` draws it.
 */

/** A note as the map needs it: its area (`null` = loose) and its birth (ms), `null` when unknown. */
export interface TerritoryNote {
  id: string;
  area: string | null;
  born: number | null;
}

/** A note's field on the axial grid. */
export interface TerritoryCell {
  id: string;
  q: number;
  r: number;
}

/** The six axial neighbours, each with the direction (degrees, screen space: y down) its shared edge faces. */
export const NEIGHBOURS: readonly { dq: number; dr: number; angle: number }[] = [
  { dq: 1, dr: 0, angle: 0 },
  { dq: 1, dr: -1, angle: -60 },
  { dq: 0, dr: -1, angle: -120 },
  { dq: -1, dr: 0, angle: 180 },
  { dq: -1, dr: 1, angle: 120 },
  { dq: 0, dr: 1, angle: 60 },
];

/** The key of a field, for lookups by position. */
export const cellKey = (q: number, r: number): string => `${q},${r}`;

/** A field's centre for a pointy-top hex of circumradius 1. */
export function axialToPixel(q: number, r: number): [number, number] {
  return [Math.sqrt(3) * (q + r / 2), 1.5 * r];
}

/** The field a point (circumradius-1 units) falls in: the cube rounding of its fractional axial position. */
export function pixelToAxial(x: number, y: number): [number, number] {
  const fq = (Math.sqrt(3) / 3) * x - y / 3;
  const fr = (2 / 3) * y;
  const fs = -fq - fr;
  let q = Math.round(fq);
  let r = Math.round(fr);
  const s = Math.round(fs);
  const dq = Math.abs(q - fq);
  const dr = Math.abs(r - fr);
  const ds = Math.abs(s - fs);
  if (dq > dr && dq > ds) q = -r - s;
  else if (dr > ds) r = -q - s;
  return [q + 0, r + 0];
}

/** How far out (as a share of the map's expected radius) the seeds of the smaller countries sit around the largest. */
const SEED_RING = 0.62;

interface Country {
  notes: TerritoryNote[];
  /** Where the country is meant to sit (circumradius-1 units): it grows towards here. */
  seedPx: [number, number];
  claimed: number;
  /** The free fields next to the country, by key, with their position — so a step parses no keys. */
  frontier: Map<string, [number, number]>;
}

const byBirth = (a: TerritoryNote, b: TerritoryNote): number =>
  (a.born ?? Infinity) - (b.born ?? Infinity) || a.id.localeCompare(b.id);

/** The free field nearest to `px` (circumradius-1 units), searched ring by ring around its own field. */
function nearestFree(px: [number, number], taken: Map<string, unknown>): [number, number] {
  const [q0, r0] = pixelToAxial(px[0], px[1]);
  for (let k = 0; ; k++) {
    let best: [number, number] | null = null;
    let bestD = Infinity;
    for (let dq = -k; dq <= k; dq++) {
      for (let dr = Math.max(-k, -dq - k); dr <= Math.min(k, -dq + k); dr++) {
        if (Math.max(Math.abs(dq), Math.abs(dr), Math.abs(dq + dr)) !== k) continue;
        const q = q0 + dq;
        const r = r0 + dr;
        if (taken.has(cellKey(q, r))) continue;
        const [x, y] = axialToPixel(q, r);
        const d = (x - px[0]) ** 2 + (y - px[1]) ** 2;
        if (d < bestD) {
          bestD = d;
          best = [q, r];
        }
      }
    }
    if (best) return best;
  }
}

/**
 * The map for a vault: one country per area, the largest in the middle and the others seeded on a ring around it,
 * each over an arc as wide as its share of the notes. They grow a field at a time — always the country furthest behind
 * its own size, always the free field next to it that lies nearest its seed — so they stay round and finish together.
 * A country walled in by its neighbours goes on in the nearest free field (an exclave) rather than stop. Deterministic:
 * the same notes give the same map.
 */
export function buildTerritory(notes: readonly TerritoryNote[]): TerritoryCell[] {
  if (notes.length === 0) return [];
  const byArea = new Map<string, TerritoryNote[]>();
  for (const n of notes) {
    const key = n.area ?? "";
    const list = byArea.get(key) ?? [];
    list.push(n);
    byArea.set(key, list);
  }
  const areas = [...byArea.entries()]
    .map(([name, list]) => ({ name, list: [...list].sort(byBirth) }))
    .sort((a, b) => b.list.length - a.list.length || a.name.localeCompare(b.name));

  // The radius a round map of all the notes would have, in circumradius-1 units (a field covers 3√3/2).
  const mapRadius = Math.sqrt((notes.length * 3 * Math.sqrt(3)) / 2 / Math.PI);
  const rest = notes.length - (areas[0]?.list.length ?? 0);
  let arc = -Math.PI / 2;
  const taken = new Map<string, number>();
  const countries: Country[] = areas.map((area, i) => {
    let seedPx: [number, number] = [0, 0];
    if (i > 0 && rest > 0) {
      const share = (area.list.length / rest) * Math.PI * 2;
      const mid = arc + share / 2;
      arc += share;
      seedPx = [Math.cos(mid) * mapRadius * SEED_RING, Math.sin(mid) * mapRadius * SEED_RING];
    }
    return { notes: area.list, seedPx, claimed: 0, frontier: new Map<string, [number, number]>() };
  });

  const cells: TerritoryCell[] = [];
  const claim = (country: Country, index: number, q: number, r: number): void => {
    const note = country.notes[country.claimed];
    if (!note) return;
    taken.set(cellKey(q, r), index);
    cells.push({ id: note.id, q, r });
    country.claimed++;
    for (const { dq, dr } of NEIGHBOURS) {
      const key = cellKey(q + dq, r + dr);
      if (!taken.has(key)) country.frontier.set(key, [q + dq, r + dr]);
    }
  };

  countries.forEach((country, i) => {
    const [q, r] = nearestFree(country.seedPx, taken);
    claim(country, i, q, r);
  });

  for (;;) {
    // The country furthest behind its own size goes next; ties go to the larger one (its index is lower).
    let next = -1;
    let lowest = Infinity;
    countries.forEach((c, i) => {
      if (c.claimed >= c.notes.length) return;
      const progress = c.claimed / c.notes.length;
      if (progress < lowest) {
        lowest = progress;
        next = i;
      }
    });
    const country = countries[next];
    if (!country) break;
    let best: [number, number] | null = null;
    let bestD = Infinity;
    // Deleting the entry being visited is safe while iterating a Map: fields a neighbour took are dropped on the way.
    for (const [key, [q, r]] of country.frontier) {
      if (taken.has(key)) {
        country.frontier.delete(key);
        continue;
      }
      const [x, y] = axialToPixel(q, r);
      const d = (x - country.seedPx[0]) ** 2 + (y - country.seedPx[1]) ** 2;
      if (d < bestD || (d === bestD && best !== null && (q < best[0] || (q === best[0] && r < best[1])))) {
        bestD = d;
        best = [q, r];
      }
    }
    const [q, r] = best ?? nearestFree(country.seedPx, taken);
    claim(country, next, q, r);
  }
  return cells;
}
