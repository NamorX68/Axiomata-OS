/** Small display helpers shared by modules. */

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "just now", "5 min ago", "3 h ago", "2 d ago" — or the date beyond a week. */
export function relativeTime(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "never";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const diff = now - t;
  if (diff < MINUTE) return "just now";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)} min ago`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)} h ago`;
  if (diff < 7 * DAY) return `${Math.floor(diff / DAY)} d ago`;
  return new Date(t).toLocaleDateString();
}

/** "in 2 h", "in 5 min", "due" — for future timestamps. */
export function untilTime(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const diff = t - now;
  if (diff <= 0) return "due";
  if (diff < MINUTE) return "in <1 min";
  if (diff < HOUR) return `in ${Math.ceil(diff / MINUTE)} min`;
  if (diff < DAY) return `in ${Math.round(diff / HOUR)} h`;
  return `in ${Math.round(diff / DAY)} d`;
}

/** Shortens a long absolute path to "…/parent/name". */
export function shortPath(path: string, keep = 2): string {
  const parts = path.split("/").filter(Boolean);
  return parts.length <= keep ? path : `…/${parts.slice(-keep).join("/")}`;
}

/** "340 B", "1.2 KB", "3.4 MB". */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/** Locale date + time, or "—". */
export function absoluteTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  return new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
