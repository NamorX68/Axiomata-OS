/**
 * Shared "read back the latest run of skill X" logic behind every
 * skill-backed connector module (`calendar`, `reminders`, and future ones —
 * see CLAUDE.md's "provider = skill, not code" decision for why a connector
 * is a skill at all). A connector module never triggers its skill on a
 * timer; it reads back whichever run happened most recently, however that
 * run was triggered — by hand from the Skills Deck, on a schedule via a
 * Routine, or the tile's own refresh button, which is just a plain
 * `run_skill` call, same mechanism.
 *
 * Split out of `core/calendar.ts` once `core/reminders.ts` needed the exact
 * same "list_runs → find newest matching skill_name → get_run" round trip —
 * two skill-backed modules reimplementing it independently is exactly the
 * duplication an architecture review flagged even at one module's own two
 * call sites (its component and its bridge action), so it's worth sharing
 * before a third connector repeats it a third time.
 */

import type { RunRecord, RunSummary } from "./backend";

/** The subset of `ModuleContext["invoke"]` this needs — spelled out by hand
 *  instead of importing `core/types` so this file stays Svelte-agnostic. */
export type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/** How far back (in `list_runs`'s newest-first order) to look for the most
 *  recent run of a given skill. */
export const RUN_LOOKUP_LIMIT = 50;

/** One skill's most recent run, resolved as far as this layer can take it —
 *  callers parse `stdout` themselves, since each skill has its own JSON
 *  contract. */
export interface LatestSkillRun {
  /** The run found, or `null` if the skill has never run at all. */
  run: RunSummary | null;
  /** The full record's captured stdout — `null` when there's no run, the
   *  run failed, or its full record couldn't be re-fetched (`error` says
   *  which). */
  stdout: string | null;
  /** The failed run's own error, or "record not found"; `null` on a clean
   *  success (including the "never run yet" case, which isn't an error). */
  error: string | null;
}

/**
 * Finds the most recent run of `skillName` (`list_runs`) and fetches its
 * full record (`get_run`) for the captured `stdout`.
 */
export async function loadLatestSkillRun(invoke: Invoke, skillName: string): Promise<LatestSkillRun> {
  const runs = await invoke<RunSummary[]>("list_runs", { limit: RUN_LOOKUP_LIMIT });
  const run = runs.find((r) => r.skill_name === skillName) ?? null;
  if (!run) return { run: null, stdout: null, error: null };
  if (run.status === "failed") {
    return { run, stdout: null, error: run.error ?? "Last run failed." };
  }
  const full = await invoke<RunRecord | null>("get_run", { id: run.id });
  if (!full) return { run, stdout: null, error: "Run record not found." };
  return { run, stdout: full.stdout, error: null };
}

/**
 * Strips one leading/trailing ` ``` ` or ` ```json ` fence from a skill's
 * stdout, if present; returns the input trimmed and unchanged otherwise.
 *
 * Every digest-producing skill's SOP asks for exactly one JSON object and
 * nothing else, but a model reply wrapping it in a code fence despite that
 * instruction is common enough in practice (seen live building both
 * `calendar-digest` and `reminders-digest`) that defensively stripping one
 * is worth it rather than failing a well-formed run over formatting.
 */
export function stripCodeFence(text: string): string {
  const trimmed = text.trim();
  const m = /^```(?:json)?\s*\n([\s\S]*?)\n?```$/.exec(trimmed);
  return m ? m[1].trim() : trimmed;
}
