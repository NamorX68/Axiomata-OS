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
  /** The newest run that was passed over because a *usable* older one exists —
   *  it failed, or its output could not be read. `null` when the run returned
   *  is the newest one. A tile uses it to say that what it shows is older than
   *  the latest run, and why. */
  skipped: SkippedRun | null;
}

/** A newer run that `loadLatestSkillRun` passed over, and why. */
export interface SkippedRun {
  run: RunSummary;
  /** A short reason: the run's own error, or what was wrong with its output. */
  reason: string;
}

/**
 * Finds the most recent *usable* run of `skillName` (`list_runs`) and
 * fetches its full record (`get_run`) for the captured `stdout`.
 *
 * "Usable" is deliberately loose: the newest run is tried first, but a run
 * that can't contribute data is skipped over in favour of the next-newest —
 * a `failed` run, a record whose full row vanished (`get_run` returned
 * nothing), a run whose stdout is empty/whitespace, or a run whose stdout
 * isn't valid JSON after fence-stripping. Connector modules parse stdout
 * as JSON — a skill's SOP promises exactly one JSON object — so a run the
 * parser would choke on (observed live as `calendar-digest` finishing with
 * an empty `result`, or a truncated `{"calendars":[],"events":[` cut off
 * mid-object) must never wipe a tile's previously-good digest; the reader
 * prefers the most recent run that produced a parseable digest and only
 * falls back to surfacing a bad run's error when no usable run exists at
 * all. The parseability check is deliberately a bare `JSON.parse` — a
 * *valid* object the connector still rejects (e.g. the skill's own
 * `{"error": "..."}` report) is the connector's job to surface as an
 * error, not something this layer should silently skip over.
 */
export async function loadLatestSkillRun(invoke: Invoke, skillName: string): Promise<LatestSkillRun> {
  const runs = await invoke<RunSummary[]>("list_runs", { limit: RUN_LOOKUP_LIMIT });
  let newestFailed: RunSummary | null = null;
  let newestMissing: RunSummary | null = null;
  let newestEmpty: RunSummary | null = null;
  let newestUnparseable: RunSummary | null = null;
  let unparseableReason: string | null = null;
  // The newest run passed over so far — runs come newest first, so the first skip is it.
  let skipped: SkippedRun | null = null;
  for (const run of runs) {
    if (run.skill_name !== skillName) continue;
    if (run.status === "failed") {
      newestFailed ??= run;
      skipped ??= { run, reason: run.error ?? "the run failed" };
      continue;
    }
    const full = await invoke<RunRecord | null>("get_run", { id: run.id });
    if (!full) {
      newestMissing ??= run;
      skipped ??= { run, reason: "its record was not found" };
      continue;
    }
    const stripped = stripCodeFence(full.stdout);
    if (!stripped) {
      newestEmpty ??= run;
      skipped ??= { run, reason: "it produced no output" };
      continue;
    }
    const object = firstJsonObject(stripped);
    let unusable = false;
    if (object === null) {
      unusable = true;
      unparseableReason ??= "no JSON object found";
    } else {
      try {
        JSON.parse(object);
      } catch (err) {
        unusable = true;
        unparseableReason ??= (err as Error).message;
      }
    }
    if (unusable) {
      newestUnparseable ??= run;
      skipped ??= { run, reason: "its output held no readable JSON" };
      continue;
    }
    return { run, stdout: full.stdout, error: null, skipped };
  }
  // No usable run at all: the error itself is what gets shown, so nothing was "skipped".
  if (newestFailed) return { run: newestFailed, stdout: null, error: newestFailed.error ?? "Last run failed.", skipped: null };
  if (newestMissing) return { run: newestMissing, stdout: null, error: "Run record not found.", skipped: null };
  if (newestUnparseable) {
    return {
      run: newestUnparseable,
      stdout: null,
      error: `${skillName} output was not valid JSON: ${unparseableReason}`,
      skipped: null,
    };
  }
  if (newestEmpty) return { run: newestEmpty, stdout: null, error: "Last run produced no output.", skipped: null };
  return { run: null, stdout: null, error: null, skipped: null };
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

/**
 * The first *balanced* JSON object in `text`, as a raw string; `null` when no
 * complete object survives to a matching `}` (a truncated `{"a":1` — the
 * "Unexpected EOF" seen live — or no `{` at all). Braces inside JSON strings
 * are skipped, so a `"{"` inside a title can't end the scan early.
 *
 * Why not just `JSON.parse(text)`? A connector skill's reply is frequently not
 * *only* the object: per the CP3 bake-off (`docs/plans/stufe2-cp3-bakeoff.md`),
 * a small model wrapped the digest in prose, or emitted a second, truncated
 * copy of the object right after the first valid one (concatenated). Parsing
 * the whole reply fails both — the first is prose, the second unparseable —
 * even though the first object itself is exactly the contract. Taking the
 * balanced first object salvages both: leading prose is skipped, and a
 * trailing duplicate is never reached. A balanced `{…}` in that prose which is
 * not JSON is skipped over, so the real object behind it is still found. Used by every digest parser and by
 * `loadLatestSkillRun`'s "is this run's output usable?" check, so the two
 * always agree on what a usable run's stdout looks like.
 */
export function firstJsonObject(text: string): string | null {
  let firstBalanced: string | null = null;
  let from = 0;
  for (;;) {
    const start = text.indexOf("{", from);
    if (start === -1) return firstBalanced;
    const end = balancedObjectEnd(text, start);
    if (end === -1) return firstBalanced; // runs off the end of the text — unterminated
    const candidate = text.slice(start, end + 1);
    if (parsesAsObject(candidate)) return candidate;
    // Balanced, but not JSON: a `{…}` in the model's prose ("format {emails: [...]}").
    // Look on after it; if nothing better turns up, the first one is handed back so
    // callers keep their own `JSON.parse` error message.
    firstBalanced ??= candidate;
    from = end + 1;
  }
}

/** The index of the `}` closing the object that opens at `start`, or `-1` when
 *  the text ends first. Braces inside strings don't count. */
function balancedObjectEnd(text: string, start: number): number {
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let i = start; i < text.length; i++) {
    const c = text[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (c === "\\") escaped = true;
      else if (c === '"') inString = false;
      continue;
    }
    if (c === '"') {
      inString = true;
    } else if (c === "{") {
      depth++;
    } else if (c === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

function parsesAsObject(candidate: string): boolean {
  try {
    const value: unknown = JSON.parse(candidate);
    return typeof value === "object" && value !== null && !Array.isArray(value);
  } catch {
    return false;
  }
}

/**
 * The skill name a connector module instance actually calls: its own
 * `config.skillName` if the owner has set one (an instance-level override —
 * this instance's settings face exposes it as a plain text field), else the
 * module's hardcoded default (`calendar-digest`, `reminders-digest`,
 * `mail-digest`). Every connector already had a fixed default constant that
 * both its refresh button and its own SOP-writing agreed on; this doesn't
 * replace that (nothing *requires* the field to be filled in), it just lets
 * an owner repoint one instance at a differently-named skill — a renamed
 * copy, an experiment — without a code change. Read at every `run_skill`
 * call site (never cached), so editing it in settings takes effect on the
 * next refresh.
 */
export function resolveSkillName(config: Record<string, unknown>, fallback: string): string {
  const override = typeof config.skillName === "string" ? config.skillName.trim() : "";
  return override || fallback;
}

/** How often a connector tile looks for a run of its skill it has not seen. */
export const WATCH_MS = 60_000;

/**
 * Calls `onNew` whenever `skillName` has a newer run than the one seen last —
 * a Routine's hourly run, one from the Skills Deck or the command line — so
 * a tile that stays mounted reads it back (and the Mail tile writes its
 * notes) instead of only on its next mount. The first look only sets the
 * baseline. One `list_runs` per `everyMs`; no agent turn. Returns the stop
 * function.
 */
export function watchSkillRuns(
  invoke: Invoke,
  skillName: () => string,
  onNew: () => void,
  everyMs: number = WATCH_MS,
): () => void {
  let seen: number | null | undefined;
  let busy = false;
  const look = async () => {
    if (busy) return;
    busy = true;
    try {
      const runs = await invoke<RunSummary[]>("list_runs", { limit: RUN_LOOKUP_LIMIT });
      const newest = runs.find((run) => run.skill_name === skillName())?.id ?? null;
      if (seen !== undefined && newest !== seen) onNew();
      seen = newest;
    } catch {
      // A failed look is retried on the next tick.
    } finally {
      busy = false;
    }
  };
  void look();
  const id = setInterval(() => void look(), everyMs);
  return () => clearInterval(id);
}
