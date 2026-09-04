/**
 * Parses the `calendar-digest` skill's JSON output and provides the pure
 * logic behind the `calendar` module — the `.svelte` shell stays thin and
 * loads / filters / renders, matching the repo's pure-logic-plus-vitest
 * convention (`core/todo.ts`, `core/snap.ts`).
 *
 * There is no live poll here (unlike `todo`'s 5 s file poll): calendar data
 * comes from an MCP tool only an agent can reach, so every refresh is a
 * `calendar-digest` skill run — a real agent turn, not a free file read. The
 * module reads back whichever run happened most recently (manually from the
 * Skills Deck, on a schedule via a Routine, or the module's own "Refresh"
 * button), it never triggers one on a timer.
 */

import type { RunSummary } from "./backend";
import { loadLatestSkillRun, stripCodeFence, type Invoke } from "./skillRun";

/** One event from the digest, already whatever the skill's SOP promises:
 *  `start`/`end` are `YYYY-MM-DD` for an all-day event, full ISO 8601
 *  otherwise. */
export interface CalendarEvent {
  title: string;
  start: string;
  end: string;
  calendar: string;
  location: string | null;
  allDay: boolean;
}

/** The skill's whole JSON payload — `calendars` lists every calendar found
 *  (even ones with no upcoming events, so the filter dropdown stays
 *  complete), `events` the upcoming events across all of them. */
export interface CalendarDigest {
  calendars: string[];
  events: CalendarEvent[];
}

/** Name of the skill the `calendar` module reads its data from. */
export const CALENDAR_SKILL_NAME = "calendar-digest";

/** An empty digest — the module's state before any run has ever happened. */
export const EMPTY_DIGEST: CalendarDigest = { calendars: [], events: [] };

/**
 * Parses a `calendar-digest` run's captured stdout into a `CalendarDigest`.
 *
 * The SOP asks for exactly one JSON object and nothing else, but a model
 * reply wrapping it in a ` ```json ` fence despite that instruction is
 * common enough in practice (seen live while building this) that stripping
 * one defensively is worth it rather than failing a well-formed run over
 * formatting. Anything else unparseable, or not an object shaped like a
 * digest, throws — the caller decides how to surface that.
 */
export function parseCalendarDigest(stdout: string): CalendarDigest {
  const stripped = stripCodeFence(stdout);
  let raw: unknown;
  try {
    raw = JSON.parse(stripped);
  } catch (err) {
    throw new Error(`calendar-digest output was not valid JSON: ${(err as Error).message}`);
  }
  if (!raw || typeof raw !== "object") {
    throw new Error("calendar-digest output was not a JSON object");
  }
  const obj = raw as Record<string, unknown>;
  if (typeof obj.error === "string") {
    throw new Error(obj.error);
  }
  const calendars = Array.isArray(obj.calendars) ? obj.calendars.filter((c): c is string => typeof c === "string") : [];
  const events = Array.isArray(obj.events) ? obj.events.filter(isCalendarEvent) : [];
  return { calendars, events };
}

function isCalendarEvent(e: unknown): e is CalendarEvent {
  if (!e || typeof e !== "object") return false;
  const o = e as Record<string, unknown>;
  return (
    typeof o.title === "string" &&
    typeof o.start === "string" &&
    typeof o.end === "string" &&
    typeof o.calendar === "string" &&
    (o.location === null || typeof o.location === "string") &&
    typeof o.allDay === "boolean"
  );
}

/** Every event whose `calendar` is `name`, or every event when `name` is
 *  `null` ("all calendars"). */
export function filterByCalendar(events: CalendarEvent[], name: string | null): CalendarEvent[] {
  return name === null ? events : events.filter((e) => e.calendar === name);
}

/** `events`, already in the skill's sorted-by-start order, grouped into
 *  runs of the same calendar day (`YYYY-MM-DD`) for an agenda list with day
 *  headers — one group per day encountered, in order, never re-merged if a
 *  day recurs non-contiguously (it shouldn't, given sorted input). */
export function groupByDay(events: CalendarEvent[]): { day: string; events: CalendarEvent[] }[] {
  const groups: { day: string; events: CalendarEvent[] }[] = [];
  for (const e of events) {
    const day = e.start.slice(0, 10);
    const last = groups[groups.length - 1];
    if (last && last.day === day) last.events.push(e);
    else groups.push({ day, events: [e] });
  }
  return groups;
}

/** "All day", "14:00–15:35", or just "14:00" when `end` doesn't parse as a
 *  same-day time (defensive — the skill always sends both). */
export function eventTimeLabel(e: CalendarEvent): string {
  if (e.allDay) return "All day";
  const start = e.start.slice(11, 16);
  const end = e.end.slice(11, 16);
  return end ? `${start}–${end}` : start;
}

export interface LatestDigest {
  /** The run this digest came from, or `null` if `calendar-digest` has
   *  never run at all. */
  run: RunSummary | null;
  digest: CalendarDigest;
  /** The failed run's own error, or a parse failure's message; `null` on
   *  a clean success (including the "never run yet" case). */
  error: string | null;
}

/**
 * Finds the most recent `calendar-digest` run — however it was triggered
 * (Skills Deck, a scheduled Routine, or a tile's own refresh) — and parses
 * its output. Thin wrapper over `skillRun.loadLatestSkillRun`, the part
 * shared with every other skill-backed connector module (`reminders`, …).
 *
 * Shared by the `calendar` module's own load and its `list` bridge action
 * (`modules/index.ts`) so the parsing step lives in exactly one place too.
 */
export async function loadLatestCalendarDigest(invoke: Invoke): Promise<LatestDigest> {
  const { run, stdout, error } = await loadLatestSkillRun(invoke, CALENDAR_SKILL_NAME);
  if (error || stdout === null) return { run, digest: EMPTY_DIGEST, error };
  try {
    return { run, digest: parseCalendarDigest(stdout), error: null };
  } catch (err) {
    return { run, digest: EMPTY_DIGEST, error: err instanceof Error ? err.message : String(err) };
  }
}
