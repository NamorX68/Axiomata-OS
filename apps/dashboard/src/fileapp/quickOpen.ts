/**
 * Quick open's matching (`docs/plans/editor.md`, ED4, W8, W14): a typed
 * query against every file's path, fuzzily — the letters in order, not
 * necessarily together — and ranked the way one expects:
 *
 * * a match inside the **file name** beats one spread over the folders;
 * * letters at a **word start** (after `/ . _ -`, a space, or a capital in
 *   camelCase) and **runs** of consecutive letters count more;
 * * **shorter** paths win a tie, and **recently opened** files come first
 *   among equals;
 * * `name:12` also asks for line 12.
 */

export interface Candidate {
  root: string;
  rel: string;
}

export interface Ranked extends Candidate {
  score: number;
  /** Indexes into `rel` of the matched letters, for highlighting. */
  positions: number[];
}

/** The query without a trailing `:line`, and that line (1-based as typed, returned zero-based). */
export function parseQuery(input: string): { text: string; line: number | null } {
  const m = /^(.*?):(\d+)$/.exec(input.trim());
  if (!m) return { text: input.trim(), line: null };
  return { text: m[1].trim(), line: Math.max(0, Number(m[2]) - 1) };
}

function isWordStart(text: string, i: number): boolean {
  if (i === 0) return true;
  const before = text[i - 1];
  if ("/._- ".includes(before)) return true;
  // camelCase: a capital after a lower-case letter.
  return text[i] !== text[i].toLowerCase() && before === before.toLowerCase() && before !== before.toUpperCase();
}

/** Matches `query` (lower case) in `text` from `from` on, greedily preferring word starts; `null` if it cannot. */
function matchIn(query: string, text: string, from: number): { score: number; positions: number[] } | null {
  const lower = text.toLowerCase();
  const positions: number[] = [];
  let score = 0;
  let at = from;
  for (const ch of query) {
    // A later word start for this letter beats the first plain occurrence.
    let found = -1;
    for (let i = at; i < lower.length; i++) {
      if (lower[i] !== ch) continue;
      if (found < 0) found = i;
      if (isWordStart(text, i)) {
        found = i;
        break;
      }
      if (positions.length > 0 && i === positions[positions.length - 1] + 1) {
        found = i;
        break;
      }
    }
    if (found < 0) return null;
    const previous = positions[positions.length - 1];
    score += 1;
    if (isWordStart(text, found)) score += 4;
    if (previous !== undefined && found === previous + 1) score += 3;
    if (previous !== undefined) score -= Math.min(3, (found - previous - 1) * 0.1);
    positions.push(found);
    at = found + 1;
  }
  return { score, positions };
}

/** How well `query` matches `rel`; `null` if the letters are not all there in order. */
export function scorePath(query: string, rel: string): { score: number; positions: number[] } | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return { score: 0, positions: [] };
  const nameStart = rel.lastIndexOf("/") + 1;
  const inName = matchIn(q, rel, nameStart);
  if (inName) {
    const exactPrefix = rel.slice(nameStart).toLowerCase().startsWith(q) ? 8 : 0;
    return { score: inName.score + 10 + exactPrefix, positions: inName.positions };
  }
  return matchIn(q, rel, 0);
}

/**
 * The best `limit` candidates for `query`. `recent` lists `root\0rel` keys,
 * most recent first; an empty query shows those recent files first, then the rest.
 */
export function rankFiles(query: string, candidates: readonly Candidate[], recent: readonly string[], limit = 50): Ranked[] {
  const recency = new Map(recent.map((key, i) => [key, recent.length - i]));
  const out: Ranked[] = [];
  for (const c of candidates) {
    const match = scorePath(query, c.rel);
    if (!match) continue;
    const boost = recency.get(`${c.root}\0${c.rel}`) ?? 0;
    out.push({ ...c, score: match.score + boost * 0.5 - c.rel.length * 0.01, positions: match.positions });
  }
  out.sort((a, b) => b.score - a.score || a.rel.localeCompare(b.rel));
  return out.slice(0, limit);
}

/** How long an index is used as it is before it is read again (W14). */
export const INDEX_FRESH_MS = 30_000;

/** An index kept per root: served at once, read again in the background once it is older than `INDEX_FRESH_MS`. */
export interface CachedIndex {
  files: string[];
  truncated: boolean;
  at: number;
}

export function isFresh(index: CachedIndex | undefined, now: number): boolean {
  return !!index && now - index.at < INDEX_FRESH_MS;
}
