/**
 * The dock's heartbeat (`docs/plans/ui-ux-round.md` UX3, G14): today as one strip, every piece of work on it as a
 * spark — skill and routine runs, and agent sessions on cards (from a card's claim to its sign-off or failure). A
 * spark's length is its duration, a failure glows red, work still going on glows. No chat turns.
 *
 * Plain logic over what the backend already answers (`list_runs`, `list_board_cards`); `shell/Heartbeat.svelte`
 * fetches and draws. The tank beside it (G15) is {@link fuel}.
 */

import { type BoardCard, type RunSummary, type SpendSummary, runFailed } from "./backend";
import type { StatusTone } from "../ui/status";

/** One spark on the strip. */
export interface Beat {
  /** Unique on the strip: `run:<id>` or `card:<id>`. */
  key: string;
  kind: "run" | "card";
  /** The run's or the card's id. */
  id: number;
  /** The board, for a card (to open it). */
  boardId?: number;
  label: string;
  /** Start and end in ms since the epoch; `end` is `null` while it still goes on. */
  start: number;
  end: number | null;
  tone: StatusTone;
}

/** The local day `now` falls in: from its midnight to the next. */
export function dayOf(now: number): { start: number; end: number } {
  const d = new Date(now);
  const start = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const end = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getTime();
  return { start, end };
}

/** A timestamp from the backend in ms, or `null` for a missing or unreadable one. */
function at(text: string | null | undefined): number | null {
  if (!text) return null;
  const ms = Date.parse(text);
  return Number.isNaN(ms) ? null : ms;
}

/** Today's skill and routine runs (every run in `list_runs` is one of those; chat turns are not runs). */
export function beatsFromRuns(runs: readonly RunSummary[], now: number): Beat[] {
  const day = dayOf(now);
  const beats: Beat[] = [];
  for (const run of runs) {
    const start = at(run.started_at);
    if (start === null || start < day.start || start >= day.end) continue;
    beats.push({
      key: `run:${run.id}`,
      kind: "run",
      id: run.id,
      label: run.source === "routine" ? `${run.skill_name} (routine)` : run.skill_name,
      start,
      end: start + Math.max(0, run.duration_ms),
      tone: run.status === "running" ? "live" : runFailed(run.status) ? "bad" : "ok",
    });
  }
  return beats;
}

/**
 * Today's agent sessions on cards: from the claim to the sign-off, the failure, the cancel or the archiving — or
 * still going on while a session for the card is open (`openCards`, from `open_card_sessions`). A session that ended
 * today but began earlier starts at midnight on the strip.
 */
export function beatsFromCards(cards: readonly BoardCard[], now: number, openCards: ReadonlySet<number>): Beat[] {
  const day = dayOf(now);
  const beats: Beat[] = [];
  for (const card of cards) {
    const claimed = at(card.claimed_at);
    if (claimed === null) continue;
    const verified = at(card.verified_at);
    const failed = at(card.failed_at);
    // Archived or taken over ends it too: a claim left on an archived card is no session at work (card #56, a week
    // "claimed", glowed across the whole day).
    const ended =
      failed ?? verified ?? at(card.canceled_at) ?? at(card.taken_over_at) ?? at(card.archived_at);
    if (ended !== null && ended < day.start) continue;
    if (claimed >= day.end) continue;
    // Only a session the backend reports open is at work; a claim without one is stale and shows nothing.
    const live = ended === null && openCards.has(card.id);
    if (ended === null && !live) continue;
    // Archived, cancelled or taken over without an outcome says nothing about when the work stopped: such a claim
    // counts only when it was made today, never stretched back to midnight.
    if (!live && failed === null && verified === null && claimed < day.start) continue;
    beats.push({
      key: `card:${card.id}`,
      kind: "card",
      id: card.id,
      boardId: card.board_id,
      label: `#${card.id} ${card.title}`,
      start: Math.max(claimed, day.start),
      end: ended,
      tone: failed !== null ? "bad" : live ? "live" : verified !== null ? "ok" : "neutral",
    });
  }
  return beats;
}

/** A spark's length in words for the list: "40 s", "12 min", "2 h 5 min"; still going on counts up to `now`. */
export function durationText(beat: Beat, now: number): string {
  const seconds = Math.max(0, Math.round(((beat.end ?? now) - beat.start) / 1000));
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const rest = minutes % 60;
  return rest === 0 ? `${Math.floor(minutes / 60)} h` : `${Math.floor(minutes / 60)} h ${rest} min`;
}

/** The card ids of an `open_card_sessions` answer; anything that is not one counts as none. */
export function openCardIds(open: unknown): Set<number> {
  const ids = new Set<number>();
  if (!Array.isArray(open)) return ids;
  for (const s of open) {
    const id = typeof s === "object" && s !== null ? (s as { card_id?: unknown }).card_id : undefined;
    if (typeof id === "number") ids.add(id);
  }
  return ids;
}

/** How far into the day of `now` a moment lies, in percent (the strip's "now" line). */
export function dayShare(ms: number, now: number): number {
  const day = dayOf(now);
  return ((ms - day.start) / (day.end - day.start)) * 100;
}

/** Where a spark sits on the strip, in percent of the day; at least `minWidth` percent wide, so a short run shows. */
export function placeOnDay(beat: Beat, now: number, minWidth = 0.6): { left: number; width: number } {
  const day = dayOf(now);
  const span = day.end - day.start;
  const end = Math.min(beat.end ?? now, day.end);
  const left = ((beat.start - day.start) / span) * 100;
  const width = Math.max(minWidth, ((end - beat.start) / span) * 100);
  // `left` is clamped so a spark at the very end still fits; the width is capped by the unclamped start, which only
  // makes it shorter, never longer than the rest of the day.
  return { left: Math.max(0, Math.min(100 - minWidth, left)), width: Math.min(width, 100 - left) };
}

/** The tank (G15): today's metered spend against the daily limit. */
export interface Fuel {
  todayUsd: number;
  /** The daily limit, or `null` when there is none (then only the amount shows). */
  capUsd: number | null;
  /** Hex segments filled, of {@link FUEL_SEGMENTS}; 0 without a limit. */
  filled: number;
  /** From 80 % of the limit on, the tank glows red. */
  hot: boolean;
}

/** How many hex segments the tank has. */
export const FUEL_SEGMENTS = 6;

/** The share of the limit at which the tank turns red (G15). */
export const FUEL_HOT_RATIO = 0.8;

/** The tank from `get_spend_summary`: the metered providers' spend today, against the one daily limit. */
export function fuel(spend: readonly SpendSummary[]): Fuel {
  const metered = spend.filter((s) => s.metered);
  const todayUsd = metered.reduce((sum, s) => sum + s.today_usd, 0);
  const capUsd = spend.find((s) => s.daily_cap_usd !== null)?.daily_cap_usd ?? null;
  if (capUsd === null || capUsd <= 0) return { todayUsd, capUsd: null, filled: 0, hot: false };
  const ratio = todayUsd / capUsd;
  const filled = Math.min(FUEL_SEGMENTS, Math.ceil(ratio * FUEL_SEGMENTS));
  return { todayUsd, capUsd, filled, hot: ratio >= FUEL_HOT_RATIO };
}
