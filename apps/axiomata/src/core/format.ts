/**
 * Small display helpers shared by modules — and the one place dates and numbers are written in the app's language
 * (`docs/plans/i18n.md`, Q12): nothing else calls `toLocale*` or `Intl` for what the owner reads.
 */

import { locale } from "../i18n/lang.svelte";
import { tFormat } from "../i18n/format";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "just now", "5 min ago", "3 h ago", "2 d ago" — or the date beyond a week. */
export function relativeTime(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return tFormat("never");
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const diff = now - t;
  if (diff < MINUTE) return tFormat("justNow");
  if (diff < HOUR) return tFormat("minutesAgo", { n: Math.floor(diff / MINUTE) });
  if (diff < DAY) return tFormat("hoursAgo", { n: Math.floor(diff / HOUR) });
  if (diff < 7 * DAY) return tFormat("daysAgo", { n: Math.floor(diff / DAY) });
  return formatDate(t);
}

/** "in 2 h", "in 5 min", "due" — for future timestamps. */
export function untilTime(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return tFormat("none");
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const diff = t - now;
  if (diff <= 0) return tFormat("due");
  if (diff < MINUTE) return tFormat("inUnderAMinute");
  if (diff < HOUR) return tFormat("inMinutes", { n: Math.ceil(diff / MINUTE) });
  if (diff < DAY) return tFormat("inHours", { n: Math.round(diff / HOUR) });
  return tFormat("inDays", { n: Math.round(diff / DAY) });
}

/** Shortens a long absolute path to "…/parent/name". */
export function shortPath(path: string, keep = 2): string {
  const parts = path.split("/").filter(Boolean);
  return parts.length <= keep ? path : `…/${parts.slice(-keep).join("/")}`;
}

/** "340 B", "1.2 KB", "3.4 MB", "2.1 GB" — in German with a decimal comma. */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  const one = (value: number) => formatNumber(value, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${one(n / 1024)} KB`;
  if (n < 1024 * 1024 * 1024) return `${one(n / (1024 * 1024))} MB`;
  return `${one(n / (1024 * 1024 * 1024))} GB`;
}

/** A number in the app's language: "1,234.5" / "1.234,5". */
export function formatNumber(n: number, options?: Intl.NumberFormatOptions): string {
  return n.toLocaleString(locale(), options);
}

type When = Date | number | string;

/** A date in the app's language; without options the short default ("30/10/2026" / "30.10.2026"). */
export function formatDate(when: When, options?: Intl.DateTimeFormatOptions): string {
  return new Date(when).toLocaleDateString(locale(), options);
}

/** A time of day in the app's language; without options hours and minutes. */
export function formatTime(
  when: When,
  options: Intl.DateTimeFormatOptions = { hour: "2-digit", minute: "2-digit" },
): string {
  return new Date(when).toLocaleTimeString(locale(), options);
}

/** A date with its time in the app's language; without options the medium date and the short time. */
export function formatDateTime(
  when: When,
  options: Intl.DateTimeFormatOptions = { dateStyle: "medium", timeStyle: "short" },
): string {
  return new Date(when).toLocaleString(locale(), options);
}

/** The names of the weekdays in the app's language, as `Intl.DateTimeFormat` writes them. */
export function weekdayFormat(width: "long" | "short" | "narrow"): Intl.DateTimeFormat {
  return new Intl.DateTimeFormat(locale(), { weekday: width });
}

/** Locale date + time, or "—". */
export function absoluteTime(iso: string | null | undefined): string {
  if (!iso) return tFormat("none");
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  return formatDateTime(t);
}

/** "Today", "Tomorrow", or a short locale weekday + date — for a
 *  `YYYY-MM-DD` day (the `calendar` module's agenda-list day headers).
 *  "Today"/"Tomorrow" are judged in **local** time (matching the events'
 *  local wall-clock dates), not UTC — otherwise near midnight the header
 *  could disagree with `core/monthGrid.ts`'s `todayIso`. */
export function dayLabel(day: string, now = Date.now()): string {
  const localIso = (d: Date) =>
    `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const base = new Date(now);
  const tomorrow = new Date(base);
  tomorrow.setDate(tomorrow.getDate() + 1);
  if (day === localIso(base)) return tFormat("today");
  if (day === localIso(tomorrow)) return tFormat("tomorrow");
  return formatDate(`${day}T00:00:00`, { weekday: "short", month: "short", day: "numeric" });
}
