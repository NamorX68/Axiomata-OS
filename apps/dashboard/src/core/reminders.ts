/**
 * Parses the `reminders-digest` skill's JSON output and provides the pure
 * logic behind the `reminders` module — the `.svelte` shell stays thin,
 * matching `core/calendar.ts`'s (and `core/todo.ts`'s) convention.
 *
 * Unlike `calendar`, there is no "all lists" view: the owner has ~12 Apple
 * Reminders lists (shopping lists, projects, gift ideas, …) with no shared
 * theme, so one combined feed would just be noise — the module always shows
 * exactly one list, and the picker has no "All" entry, only real list names.
 * Same no-live-poll shape as `calendar` otherwise: reminders data sits
 * behind an MCP tool only an agent can reach, so every refresh is a real
 * `reminders-digest` run, never a timer.
 */

import type { RunSummary } from "./backend";
import { loadLatestSkillRun, stripCodeFence, type Invoke } from "./skillRun";

/** One reminders priority, normalised by the skill's SOP onto exactly one
 *  of these four (whatever the underlying tool actually returns). */
export type ReminderPriority = "none" | "low" | "medium" | "high";

/** One open (incomplete) reminder — the skill's SOP only ever reports
 *  open tasks, so there is no `completed` field to check. */
export interface ReminderTask {
  title: string;
  list: string;
  notes: string | null;
  /** ISO 8601, or `YYYY-MM-DD` for a date-only due date; `null` if unset. */
  dueDate: string | null;
  priority: ReminderPriority;
}

/** The skill's whole JSON payload — `lists` names every reminder list found
 *  (even ones with no open tasks, so the picker stays complete), `tasks`
 *  every open task across all of them. */
export interface ReminderDigest {
  lists: string[];
  tasks: ReminderTask[];
}

/** Name of the skill the `reminders` module reads its data from. */
export const REMINDERS_SKILL_NAME = "reminders-digest";

/** An empty digest — the module's state before any run has ever happened. */
export const EMPTY_REMINDER_DIGEST: ReminderDigest = { lists: [], tasks: [] };

const PRIORITIES: readonly ReminderPriority[] = ["none", "low", "medium", "high"];

/**
 * Parses a `reminders-digest` run's captured stdout into a `ReminderDigest`.
 * Same defensive shape as `parseCalendarDigest`: strips a stray ` ```json `
 * fence, surfaces the skill's own `{"error": "..."}` text, and drops
 * malformed entries instead of failing the whole digest over one bad task.
 */
export function parseReminderDigest(stdout: string): ReminderDigest {
  const stripped = stripCodeFence(stdout);
  let raw: unknown;
  try {
    raw = JSON.parse(stripped);
  } catch (err) {
    throw new Error(`reminders-digest output was not valid JSON: ${(err as Error).message}`);
  }
  if (!raw || typeof raw !== "object") {
    throw new Error("reminders-digest output was not a JSON object");
  }
  const obj = raw as Record<string, unknown>;
  if (typeof obj.error === "string") {
    throw new Error(obj.error);
  }
  const lists = Array.isArray(obj.lists) ? obj.lists.filter((l): l is string => typeof l === "string") : [];
  const tasks = Array.isArray(obj.tasks) ? obj.tasks.filter(isReminderTask) : [];
  return { lists, tasks };
}

function isReminderTask(t: unknown): t is ReminderTask {
  if (!t || typeof t !== "object") return false;
  const o = t as Record<string, unknown>;
  return (
    typeof o.title === "string" &&
    typeof o.list === "string" &&
    (o.notes === null || typeof o.notes === "string") &&
    (o.dueDate === null || typeof o.dueDate === "string") &&
    typeof o.priority === "string" &&
    PRIORITIES.includes(o.priority as ReminderPriority)
  );
}

/** Every open task on `list` — there is no "all lists" option, `list` is
 *  always one real name (see this module's own doc comment for why). */
export function tasksForList(tasks: ReminderTask[], list: string): ReminderTask[] {
  return tasks.filter((t) => t.list === list);
}

/** Which list to show when nothing has been picked yet, or the previously
 *  picked list no longer exists in this digest — the first one
 *  alphabetically, so the choice is stable across runs rather than
 *  depending on the skill's own list order. `null` only when the digest
 *  has no lists at all. */
export function defaultList(lists: string[]): string | null {
  if (lists.length === 0) return null;
  return [...lists].sort((a, b) => a.localeCompare(b))[0];
}

export interface LatestReminderDigest {
  /** The run this digest came from, or `null` if `reminders-digest` has
   *  never run at all. */
  run: RunSummary | null;
  digest: ReminderDigest;
  /** The failed run's own error, or a parse failure's message; `null` on
   *  a clean success (including the "never run yet" case). */
  error: string | null;
}

/**
 * Finds the most recent `reminders-digest` run — however it was triggered
 * (Skills Deck, a scheduled Routine, or a tile's own refresh) — and parses
 * its output. Same shared "find the latest run" round trip as `calendar`
 * (`core/skillRun.ts`), just parsed with this module's own contract.
 */
export async function loadLatestReminderDigest(invoke: Invoke): Promise<LatestReminderDigest> {
  const { run, stdout, error } = await loadLatestSkillRun(invoke, REMINDERS_SKILL_NAME);
  if (error || stdout === null) return { run, digest: EMPTY_REMINDER_DIGEST, error };
  try {
    return { run, digest: parseReminderDigest(stdout), error: null };
  } catch (err) {
    return { run, digest: EMPTY_REMINDER_DIGEST, error: err instanceof Error ? err.message : String(err) };
  }
}
