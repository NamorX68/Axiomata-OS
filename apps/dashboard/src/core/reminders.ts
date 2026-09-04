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
import { buildToolCallInstruction, quoteForInstruction, runInstructWrite } from "./instruct";
import { loadLatestSkillRun, stripCodeFence, type Invoke } from "./skillRun";

/** One reminders priority, normalised by the skill's SOP onto exactly one
 *  of these four (whatever the underlying tool actually returns). */
export type ReminderPriority = "none" | "low" | "medium" | "high";

/** One open (incomplete) reminder — the skill's SOP only ever reports open
 *  tasks, so there is no `completed` field to check; completing one is what
 *  removes it from view. `id` is the reminders_tasks tool's own identifier,
 *  needed to target this exact task for complete/delete. */
export interface ReminderTask {
  id: string;
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
    typeof o.id === "string" &&
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

/** The MCP tool a reminders write instruction needs `allowedTools` scoped
 *  to — the same tool `reminders-digest` reads with, but write actions
 *  never need `reminders_lists`, so this is narrower. */
export const REMINDERS_WRITE_TOOL = "mcp__apple-reminders__reminders_tasks";

/** Form input for creating one task — the module's create form maps onto
 *  this directly. */
export interface NewReminderTask {
  title: string;
  list: string;
  /** `YYYY-MM-DD` or `YYYY-MM-DD HH:mm:ss`; `null` for no due date. */
  dueDate: string | null;
  notes: string | null;
}

/**
 * Creates one reminder via a one-shot instruct turn (the `reminders_tasks`
 * MCP tool's `create` action) and returns the new `ReminderTask` — built
 * from `input` plus the id the agent reports back, so the caller can
 * insert it into its own state without waiting on a full (and, for a list
 * this size, possibly very slow) digest re-run.
 *
 * Errors:
 *   Throws on an agent-reported failure, or when the reply doesn't look
 *   like a plausible id (`runInstructWrite`'s `looksLikePlausibleId`) —
 *   the task may still have been created even so, the caller's error
 *   message should say to check with ↻.
 */
export async function createReminderTask(invoke: Invoke, input: NewReminderTask): Promise<ReminderTask> {
  const params = [`action="create"`, `title=${quoteForInstruction(input.title)}`, `targetList=${quoteForInstruction(input.list)}`];
  if (input.dueDate) params.push(`dueDate=${quoteForInstruction(input.dueDate)}`);
  if (input.notes) params.push(`note=${quoteForInstruction(input.notes)}`);
  const instruction = buildToolCallInstruction("reminders_tasks", REMINDERS_WRITE_TOOL, params, { kind: "id" });
  const id = await runInstructWrite(invoke, instruction, REMINDERS_WRITE_TOOL);
  return { id, title: input.title, list: input.list, notes: input.notes, dueDate: input.dueDate, priority: "none" };
}

/** Marks one reminder complete (removing it from every future digest, the
 *  same way finishing it in Reminders.app would) via a one-shot instruct
 *  turn.
 *
 * Errors:
 *   Throws on an agent-reported failure, or when the reply isn't exactly
 *   `"OK"` — see `deleteReminderTask`'s doc for why that check matters
 *   here specifically (no poll to self-correct a silently-failed write). */
export async function completeReminderTask(invoke: Invoke, id: string): Promise<void> {
  const instruction = buildToolCallInstruction(
    "reminders_tasks",
    REMINDERS_WRITE_TOOL,
    [`action="update"`, `id=${quoteForInstruction(id)}`, `completed=true`],
    { kind: "literal", value: "OK" },
  );
  await runInstructWrite(invoke, instruction, REMINDERS_WRITE_TOOL);
}

/** Deletes one reminder by id via a one-shot instruct turn.
 *
 * Errors:
 *   Throws on an agent-reported failure, or when the reply isn't exactly
 *   `"OK"` — `runInstructWrite` verifies the reply itself, not just that
 *   the turn didn't error, since neither `calendar` nor `reminders` polls
 *   to self-correct a stale local-state patch from a write that silently
 *   didn't happen. */
export async function deleteReminderTask(invoke: Invoke, id: string): Promise<void> {
  const instruction = buildToolCallInstruction(
    "reminders_tasks",
    REMINDERS_WRITE_TOOL,
    [`action="delete"`, `id=${quoteForInstruction(id)}`],
    { kind: "literal", value: "OK" },
  );
  await runInstructWrite(invoke, instruction, REMINDERS_WRITE_TOOL);
}
