/**
 * The calendar's day arc (`docs/plans/ui-ux-round.md` UX3, G11): the waking day as the sun's path, a half circle from
 * {@link ARC_FROM_HOUR} on the left to {@link ARC_TO_HOUR} on the right, with today's events as segments on it and
 * "now" as a glowing hexagon. Plain geometry here; `modules/DayArc.svelte` draws it.
 *
 * The arc lives in a 100 × 58 SVG box: centre (50, 52), radius 44.
 */

import type { CalendarEvent } from "./calendar";

export const ARC_FROM_HOUR = 6;
export const ARC_TO_HOUR = 22;

const CX = 50;
const CY = 52;
const R = 44;

/** How far along the arc a moment lies, 0 (the arc's start hour) … 1 (its end hour), clamped. */
export function arcFraction(ms: number): number {
  const d = new Date(ms);
  const hours = d.getHours() + d.getMinutes() / 60 + d.getSeconds() / 3600;
  return Math.min(1, Math.max(0, (hours - ARC_FROM_HOUR) / (ARC_TO_HOUR - ARC_FROM_HOUR)));
}

/** The point on the arc at a fraction: the left end at 0, the top at ½, the right end at 1. */
export function arcPoint(fraction: number): { x: number; y: number } {
  const angle = Math.PI * (1 - fraction);
  return { x: CX + R * Math.cos(angle), y: CY - R * Math.sin(angle) };
}

/** An SVG path along the arc between two fractions (`from` ≤ `to`). */
export function arcPath(from: number, to: number): string {
  const a = arcPoint(from);
  const b = arcPoint(Math.max(from, to));
  const f = (n: number): string => n.toFixed(2);
  return `M ${f(a.x)} ${f(a.y)} A ${R} ${R} 0 0 1 ${f(b.x)} ${f(b.y)}`;
}

/** One event as a segment of the arc. */
export interface ArcSegment {
  id: string;
  title: string;
  from: number;
  to: number;
  /** It is going on right now. */
  live: boolean;
}

/**
 * Today's timed events as arc segments (all-day events have no place on a clock); outside the arc's hours, none. An
 * event reaching into today from yesterday starts at the arc's beginning, one running on past midnight ends at its end.
 */
export function arcSegments(events: readonly CalendarEvent[], now: number): ArcSegment[] {
  const d = new Date(now);
  const dayStart = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dayEnd = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getTime();
  const segments: ArcSegment[] = [];
  for (const e of events) {
    if (e.allDay) continue;
    const start = Date.parse(e.start);
    const end = Date.parse(e.end);
    if (Number.isNaN(start) || Number.isNaN(end) || end <= dayStart || start >= dayEnd) continue;
    const from = start < dayStart ? 0 : arcFraction(start);
    const to = end >= dayEnd ? 1 : arcFraction(end);
    if (to <= from) continue;
    segments.push({ id: e.id, title: e.title, from, to, live: start <= now && now < end });
  }
  return segments;
}
