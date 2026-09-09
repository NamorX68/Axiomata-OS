/**
 * Pure date logic behind the `calendar` module's mini-month widget
 * (`modules/MiniCalendar.svelte`). Same pure-logic-plus-vitest split as
 * `core/routineInterval.ts` / `core/todo.ts` — the `.svelte` shell only
 * renders and emits.
 *
 * Everything here works in **local** calendar dates as `YYYY-MM-DD`
 * strings, never UTC: a calendar shows the user's own days, and the
 * `calendar-digest` events it lines up against carry local wall-clock
 * dates. Date arithmetic goes through `Date`'s component setters
 * (`setDate`), so it stays correct across DST and month/year boundaries.
 */

/** One cell of the 6×7 month grid. */
export interface DayCell {
  /** `YYYY-MM-DD` (local). */
  iso: string;
  /** `true` for a day of the displayed month, `false` for the leading /
   *  trailing filler days from the adjacent months. */
  inMonth: boolean;
  /** `true` for the local "today" (relative to the `now` passed in). */
  isToday: boolean;
}

/** A month laid out for display: always 6 rows × 7 columns, Monday-first. */
export interface MonthGrid {
  /** `year`/`month` of the displayed month (`month` is 1–12). */
  year: number;
  month: number;
  /** Localised "September 2026". */
  label: string;
  /** 6 weeks, each 7 [`DayCell`]s, Monday → Sunday. */
  weeks: DayCell[][];
}

/** A displayed month, `month` 1–12. */
export interface YearMonth {
  year: number;
  month: number;
}

/** Local `YYYY-MM-DD` for a `Date`. */
export function isoDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** A `Date` at local midnight for a `YYYY-MM-DD` string. */
export function parseIso(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** The local "today" as `YYYY-MM-DD`. */
export function todayIso(now: number = Date.now()): string {
  return isoDate(new Date(now));
}

/** `iso` shifted by `n` days (negative allowed), DST-safe. */
export function addDays(iso: string, n: number): string {
  const d = parseIso(iso);
  d.setDate(d.getDate() + n);
  return isoDate(d);
}

/** `{ year, month }` of a `YYYY-MM-DD` day. */
export function monthOf(iso: string): YearMonth {
  const [year, month] = iso.split("-").map(Number);
  return { year, month };
}

/** A displayed month shifted by `delta` months (negative allowed),
 *  rolling the year over. */
export function shiftMonth(ym: YearMonth, delta: number): YearMonth {
  // month is 1–12; work in a 0-based absolute month count, then back.
  const abs = ym.year * 12 + (ym.month - 1) + delta;
  return { year: Math.floor(abs / 12), month: (abs % 12) + 1 };
}

/** The `len` consecutive days starting at `iso` — the agenda's window
 *  ("selected day … selected day + len-1"). */
export function weekRange(iso: string, len = 7): string[] {
  return Array.from({ length: len }, (_, i) => addDays(iso, i));
}

/** Monday-first index (0 = Monday … 6 = Sunday) for a `Date`. */
function mondayIndex(d: Date): number {
  return (d.getDay() + 6) % 7;
}

/**
 * Builds the [`MonthGrid`] for `year`/`month` (1–12). The grid always spans
 * exactly 6 weeks so the widget's height never jumps between months; the
 * first cell is the Monday on or before the 1st, filler days carry
 * `inMonth: false`. `today` (a `YYYY-MM-DD` string) is the day to flag —
 * passed explicitly so the caller controls "now" (defaults to the real
 * local today).
 */
export function monthGrid(year: number, month: number, today: string = todayIso()): MonthGrid {
  const first = new Date(year, month - 1, 1);
  const start = new Date(year, month - 1, 1 - mondayIndex(first));

  const weeks: DayCell[][] = [];
  const cursor = new Date(start);
  for (let w = 0; w < 6; w++) {
    const week: DayCell[] = [];
    for (let d = 0; d < 7; d++) {
      const iso = isoDate(cursor);
      week.push({ iso, inMonth: cursor.getMonth() === month - 1, isToday: iso === today });
      cursor.setDate(cursor.getDate() + 1);
    }
    weeks.push(week);
  }

  return {
    year,
    month,
    label: first.toLocaleDateString(undefined, { month: "long", year: "numeric" }),
    weeks,
  };
}

/** Seven weekday labels, Monday-first, in the runtime locale — e.g.
 *  `["Mo","Di","Mi","Do","Fr","Sa","So"]` with `"short"`. */
export function weekdayLabels(width: "short" | "narrow" = "short"): string[] {
  const fmt = new Intl.DateTimeFormat(undefined, { weekday: width });
  // 2024-01-01 is a Monday.
  return Array.from({ length: 7 }, (_, i) => fmt.format(new Date(2024, 0, 1 + i)));
}
