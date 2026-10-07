/**
 * The model behind the ⌘K spotlight (`docs/plans/spotlight-search.md`, CP1): which items match a query, in which order,
 * and where a group header goes. Pure — it takes plain lists the caller already fetched and imports nothing from the
 * app, so the ranking is tested without a backend, a bus or a DOM. What a chosen item *does* is the caller's business
 * (CP2): an item carries its `kind` and an opaque `payload` to act on, never a closure.
 */

/** What a spotlight item is; the order of [`KIND_ORDER`] is the order of the groups in the list. */
export type SpotlightKind = "command" | "module" | "skill" | "routine" | "card" | "plan" | "project" | "file";

/** A result group: a kind, or the short "jump to" list shown for an empty query. */
export type SpotlightGroup = SpotlightKind | "suggestion";

export interface SpotlightItem {
  /** Unique within its kind; the caller maps it back to what it stands for. */
  id: string;
  kind: SpotlightKind;
  title: string;
  /** Shown under the title and matched too, but weighs less than a title hit. */
  subtitle?: string;
  /** Matched like the subtitle, never shown (a skill's description, a card's labels). */
  keywords?: string;
  /**
   * Set on a file the full-text search found: how often the query occurs in its content. Such a file whose *name* does
   * not match ranks below every item that matches by name.
   */
  contentMatches?: number;
  /** What the caller needs to act on the item; opaque here. */
  payload?: unknown;
}

/** The raw lists a ranking works on. `suggestions` is what an empty query shows, in the order given. */
export type SpotlightSources = Partial<Record<SpotlightKind, SpotlightItem[]>> & { suggestions?: SpotlightItem[] };

export interface SpotlightRow {
  item: SpotlightItem;
  group: SpotlightGroup;
  /** True on the first row of a group: the list draws the group's header above it. */
  groupStart: boolean;
  score: number;
}

/** The groups' order in the list: what is quickest to act on first, files — the long tail — last. */
export const KIND_ORDER: readonly SpotlightKind[] = [
  "command",
  "module",
  "skill",
  "routine",
  "card",
  "plan",
  "project",
  "file",
];

export const GROUP_LABEL: Record<SpotlightGroup, string> = {
  suggestion: "Springe zu",
  command: "Befehle",
  module: "Module",
  skill: "Skills",
  routine: "Routinen",
  card: "Karten",
  plan: "Pläne",
  project: "Projekte",
  file: "Dateien",
};

/** Rows in the list in all. */
export const MAX_ROWS = 20;

/** Rows of one kind: a board with hundreds of cards must not push every other group out of the list. */
export const MAX_PER_KIND = 8;

// Score tiers, spaced so that the position penalty (at most `POSITION_CAP` / 2) never lifts a hit into the next tier.
const EXACT = 1000;
const PREFIX = 800;
const WORD_START = 600;
const SUBSTRING = 400;
const SUBSEQUENCE = 200;
const POSITION_CAP = 100;

/** A query this short matches only as text; as scattered letters it would match almost anything. */
const MIN_SUBSEQUENCE_LENGTH = 3;

/** How far apart a scattered match's letters may be before it stops mattering; keeps it above [`CONTENT_MAX`]. */
const SPREAD_CAP = 90;

/** What a field other than the title is worth next to the title: a subtitle hit alone is a weaker answer. */
const SECONDARY_WEIGHT = 0.35;

/** A content-only file hit scores between these bounds — strictly below the weakest name hit (`SUBSEQUENCE - SPREAD_CAP`). */
const CONTENT_BASE = 100;
const CONTENT_MAX = 105;

/** Lower case with the accents stripped, so that "Pläne" is found by "plane" and "Café" by "cafe". */
export function normalize(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

const isWordChar = (char: string): boolean => /[a-z0-9]/.test(char);

/** The score of one already-normalised word `query` against one already-normalised `text`; `null` for no match. */
function scoreWord(query: string, text: string): number | null {
  if (text === query) return EXACT;
  if (text.startsWith(query)) return PREFIX;
  let wordStart = -1;
  let first = -1;
  for (let from = text.indexOf(query); from !== -1; from = text.indexOf(query, from + 1)) {
    if (first === -1) first = from;
    if (from > 0 && !isWordChar(text[from - 1])) {
      wordStart = from;
      break;
    }
  }
  if (wordStart !== -1) return WORD_START - Math.min(wordStart, POSITION_CAP) / 2;
  if (first !== -1) return SUBSTRING - Math.min(first, POSITION_CAP) / 2;
  if (query.length < MIN_SUBSEQUENCE_LENGTH) return null;
  let at = 0;
  let begin = -1;
  for (const char of query) {
    at = text.indexOf(char, at);
    if (at === -1) return null;
    if (begin === -1) begin = at;
    at += 1;
  }
  const spread = at - begin - query.length;
  return SUBSEQUENCE - Math.min(spread, SPREAD_CAP);
}

/**
 * How well `text` answers `query`, or `null` when it does not. A query of several words matches when every word does
 * (in any order); its score is the mean of the words' scores. Case- and accent-insensitive.
 */
export function scoreMatch(query: string, text: string): number | null {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return null;
  const haystack = normalize(text);
  let total = 0;
  for (const word of words) {
    const score = scoreWord(word, haystack);
    if (score === null) return null;
    total += score;
  }
  return total / words.length;
}

/** The score of one item for `query`, or `null` when it does not match at all. */
function scoreItem(query: string, item: SpotlightItem): number | null {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  const title = normalize(item.title);
  const secondary = normalize([item.subtitle, item.keywords].filter(Boolean).join(" "));
  let total = 0;
  for (const word of words) {
    const inTitle = scoreWord(word, title);
    const inSecondary = secondary === "" ? null : scoreWord(word, secondary);
    const best = Math.max(inTitle ?? -1, inSecondary === null ? -1 : inSecondary * SECONDARY_WEIGHT);
    if (best < 0) {
      // Only the full-text search can answer for a word that is in no name: the content it found.
      if (item.contentMatches === undefined) return null;
      return Math.min(CONTENT_BASE + item.contentMatches / 10, CONTENT_MAX);
    }
    total += best;
  }
  // Of two equal scores the shorter title is the closer answer.
  return total / words.length - title.length / 1000;
}

const byScore = (a: SpotlightRow, b: SpotlightRow): number =>
  b.score - a.score || a.item.title.localeCompare(b.item.title) || a.item.id.localeCompare(b.item.id);

/** Rows in the order of `KIND_ORDER`, best first within a kind, each group's first row marked. */
function grouped(rows: SpotlightRow[]): SpotlightRow[] {
  const out: SpotlightRow[] = [];
  for (const kind of KIND_ORDER) {
    const own = rows.filter((row) => row.group === kind).sort(byScore);
    own.forEach((row, index) => out.push({ ...row, groupStart: index === 0 }));
  }
  return out;
}

/**
 * The list for `query`: every matching item of every kind, at most [`MAX_PER_KIND`] of a kind and [`MAX_ROWS`] in all
 * (the best by score, whatever their kind), then laid out in group order. An empty query yields the suggestions as
 * given. A query that matches nothing yields `[]`.
 */
export function rankItems(query: string, sources: SpotlightSources): SpotlightRow[] {
  if (query.trim() === "") {
    return (sources.suggestions ?? [])
      .slice(0, MAX_ROWS)
      .map((item, index) => ({ item, group: "suggestion", groupStart: index === 0, score: 0 }));
  }
  const kept: SpotlightRow[] = [];
  for (const kind of KIND_ORDER) {
    const rows: SpotlightRow[] = [];
    for (const item of sources[kind] ?? []) {
      const score = scoreItem(query, item);
      if (score !== null) rows.push({ item, group: kind, groupStart: false, score });
    }
    kept.push(...rows.sort(byScore).slice(0, MAX_PER_KIND));
  }
  return grouped(kept.sort(byScore).slice(0, MAX_ROWS));
}
