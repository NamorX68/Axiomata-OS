/**
 * A friendlier front end for a routine's cron expression: a small closed set
 * of "every N minutes" / "every N hours" / "daily at" / "weekly on" shapes
 * that compile to the 6-field, seconds-first cron the core actually stores
 * (see `crates/axiomata-core/src/routines/schedule.rs`), plus a `"custom"`
 * escape hatch for anything the picker doesn't cover — the raw cron field
 * never goes away, it just stops being the default way in.
 *
 * `cronToInterval` only recognises the exact shapes `intervalToCron`
 * generates. A cron expression from anywhere else (hand-typed, imported, or
 * a more exotic schedule) round-trips to `{ kind: "custom" }` rather than a
 * best-effort guess — the raw text is always shown back verbatim in that
 * case, so nothing is ever silently reinterpreted.
 */

/** Cron day-of-week tokens, index 0 = Sunday .. 6 = Saturday — the `cron`
 *  crate accepts these three-letter names directly (already exercised by a
 *  core test), which sidesteps the numeric 0-vs-1-for-Sunday ambiguity
 *  between crontab dialects. */
export const WEEKDAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;

export type RoutineInterval =
  | { kind: "minutes"; everyMinutes: number }
  | { kind: "hourly"; everyHours: number; minute: number }
  | { kind: "daily"; hour: number; minute: number }
  | { kind: "weekly"; weekday: number; hour: number; minute: number }
  | { kind: "custom"; cron: string };

/** The picker's starting point for a brand-new routine: once a day at 9am. */
export function defaultInterval(): RoutineInterval {
  return { kind: "daily", hour: 9, minute: 0 };
}

function clamp(n: number, min: number, max: number): number {
  if (!Number.isFinite(n)) return min;
  return Math.min(max, Math.max(min, Math.round(n)));
}

/** Compiles a [[RoutineInterval]] into the cron expression `NewRoutine`
 *  expects. Out-of-range numeric fields are clamped rather than producing
 *  an invalid expression the core would then reject. */
export function intervalToCron(interval: RoutineInterval): string {
  switch (interval.kind) {
    case "minutes":
      return `0 */${clamp(interval.everyMinutes, 1, 59)} * * * *`;
    case "hourly":
      return `0 ${clamp(interval.minute, 0, 59)} */${clamp(interval.everyHours, 1, 23)} * * *`;
    case "daily":
      return `0 ${clamp(interval.minute, 0, 59)} ${clamp(interval.hour, 0, 23)} * * *`;
    case "weekly":
      return `0 ${clamp(interval.minute, 0, 59)} ${clamp(interval.hour, 0, 23)} * * ${WEEKDAY_NAMES[clamp(interval.weekday, 0, 6)]}`;
    case "custom":
      return interval.cron.trim();
  }
}

/** Reverses [[intervalToCron]] for prefilling the picker when editing an
 *  existing routine. Falls back to `{ kind: "custom", cron }` for any
 *  expression that isn't exactly one of the four generated shapes. */
export function cronToInterval(cron: string): RoutineInterval {
  const c = cron.trim();

  let m = /^0 \*\/([0-9]{1,2}) \* \* \* \*$/.exec(c);
  if (m) {
    const everyMinutes = Number(m[1]);
    if (everyMinutes >= 1 && everyMinutes <= 59) return { kind: "minutes", everyMinutes };
  }

  m = /^0 ([0-9]{1,2}) \*\/([0-9]{1,2}) \* \* \*$/.exec(c);
  if (m) {
    const minute = Number(m[1]);
    const everyHours = Number(m[2]);
    if (minute >= 0 && minute <= 59 && everyHours >= 1 && everyHours <= 23) {
      return { kind: "hourly", everyHours, minute };
    }
  }

  m = /^0 ([0-9]{1,2}) ([0-9]{1,2}) \* \* \*$/.exec(c);
  if (m) {
    const minute = Number(m[1]);
    const hour = Number(m[2]);
    if (minute >= 0 && minute <= 59 && hour >= 0 && hour <= 23) return { kind: "daily", hour, minute };
  }

  m = /^0 ([0-9]{1,2}) ([0-9]{1,2}) \* \* (Sun|Mon|Tue|Wed|Thu|Fri|Sat)$/.exec(c);
  if (m) {
    const minute = Number(m[1]);
    const hour = Number(m[2]);
    const weekday = WEEKDAY_NAMES.indexOf(m[3] as (typeof WEEKDAY_NAMES)[number]);
    if (minute >= 0 && minute <= 59 && hour >= 0 && hour <= 23) return { kind: "weekly", weekday, hour, minute };
  }

  return { kind: "custom", cron: c };
}

/** A short human summary of an interval, for the routines board's tooltip
 *  and the picker's own preview line — e.g. "every 15 min", "daily at 09:00",
 *  "weekly on Mon at 09:00". */
export function describeInterval(interval: RoutineInterval): string {
  const hhmm = (h: number, m: number) => `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  switch (interval.kind) {
    case "minutes":
      return `every ${interval.everyMinutes} min`;
    case "hourly":
      return interval.everyHours === 1 ? `hourly at :${String(interval.minute).padStart(2, "0")}` : `every ${interval.everyHours}h at :${String(interval.minute).padStart(2, "0")}`;
    case "daily":
      return `daily at ${hhmm(interval.hour, interval.minute)}`;
    case "weekly":
      return `weekly on ${WEEKDAY_NAMES[interval.weekday]} at ${hhmm(interval.hour, interval.minute)}`;
    case "custom":
      return interval.cron;
  }
}
