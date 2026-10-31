<!--
  The calendar's day arc (`docs/plans/ui-ux-round.md` UX3, G11), in place of the analogue clock: the waking day as a
  half circle, the part already gone a shade darker, today's events as segments (the one going on glows), "now" as a
  hexagon on the arc and the time in the middle. Geometry in `core/dayArc.ts`.
-->
<script lang="ts">
  import { formatTime } from "../core/format";
  import type { CalendarEvent } from "../core/calendar";
  import { ARC_FROM_HOUR, ARC_TO_HOUR, arcFraction, arcPath, arcPoint, arcSegments } from "../core/dayArc";
  import { tModules } from "../i18n/modules";

  interface Props {
    /** The events the calendar shows; the arc takes today's timed ones. */
    events: readonly CalendarEvent[];
    /** Rendered height in px, the mini-month's, so the two line up. */
    size: number;
    /** The clock's time, from the parent's timer. */
    now: Date;
  }

  let { events, size, now }: Props = $props();

  const uid = $props.id();
  /** The glow filter's id, unique per arc. */
  const glowId = `day-arc-glow-${uid}`;

  const at = $derived(now.getTime());
  const nowAt = $derived(arcFraction(at));
  const here = $derived(arcPoint(nowAt));
  const outside = $derived(now.getHours() < ARC_FROM_HOUR || now.getHours() >= ARC_TO_HOUR);
  const segments = $derived(arcSegments(events, at));
  const time = $derived(formatTime(now));
  /** Read out: the time and today's events by name, since their titles are otherwise only tooltips. */
  const label = $derived(
    segments.length === 0
      ? tModules("arcEmpty", { time, from: ARC_FROM_HOUR, to: ARC_TO_HOUR })
      : tModules("arcToday", { time, events: segments.map((s) => s.title).join(", ") }),
  );

  /** A small hexagon around a point, as SVG points. */
  function hex(cx: number, cy: number, r: number): string {
    return Array.from({ length: 6 }, (_, i) => {
      const a = (Math.PI / 3) * i - Math.PI / 2;
      return `${(cx + r * Math.cos(a)).toFixed(2)},${(cy + r * Math.sin(a)).toFixed(2)}`;
    }).join(" ");
  }
</script>

<!-- Takes the room beside the mini-month, from a little narrower than it is tall up to its full width: the analogue face
     it replaced was square, and a wider arc wrapped under the month in a tile of the usual width (owner, 2026-10-11). -->
<div
  class="clock arc"
  style:height="{size}px"
  style:min-width="{Math.round(size * 0.6)}px"
  style:max-width="{Math.round(size * 1.6)}px"
>
  <svg viewBox="0 0 100 62" role="img" aria-label={label}>
    <!-- An SVG filter, not a CSS one: WebKit does not reliably draw CSS filters on SVG children. -->
    <defs>
      <filter id={glowId} x="-50%" y="-50%" width="200%" height="200%">
        <feDropShadow class="shine" dx="0" dy="0" stdDeviation="1.4" />
      </filter>
    </defs>
    <path class="track" d={arcPath(0, 1)} />
    {#if nowAt > 0}<path class="gone" d={arcPath(0, nowAt)} />{/if}
    {#each segments as s (s.id)}
      <path
        class="event"
        class:live={s.live}
        d={arcPath(s.from, s.to)}
        filter={s.live ? `url(#${glowId})` : undefined}><title>{s.title}</title></path
      >
    {/each}
    <!-- The arc's hours at its ends: without them the hexagon resting at an end after ten at night reads as a
         riddle (owner, 2026-10-11). -->
    <text class="hour" x="6" y="61" text-anchor="middle">{ARC_FROM_HOUR}</text>
    <text class="hour" x="94" y="61" text-anchor="middle">{ARC_TO_HOUR}</text>
    <!-- Outside the arc's hours "now" waits at the nearer end, quiet: the day on the arc is over or not begun. -->
    <polygon
      class="now"
      class:outside
      points={hex(here.x, here.y, 3.2)}
      filter={outside ? undefined : `url(#${glowId})`}
    />
    <text class="time" x="50" y="46" text-anchor="middle">{time}</text>
  </svg>
</div>

<style>
  .clock.arc {
    flex: 1 1 0;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
  }

  svg {
    width: 100%;
    height: 100%;
    overflow: visible;
  }

  path {
    fill: none;
    stroke-linecap: round;
  }

  .track {
    stroke: var(--ax-border-strong);
    stroke-width: 1;
  }

  .gone {
    stroke: var(--ax-ash);
    stroke-width: 1.6;
  }

  .event {
    stroke: var(--ax-text-muted);
    stroke-width: 4;
  }

  /* The event going on right now: the one glowing segment (the accent rule). */
  .event.live {
    stroke: var(--ax-glow);
    animation: var(--ax-anim-breathe);
  }

  .now {
    fill: var(--ax-glow);
  }

  .now.outside {
    fill: var(--ax-text-muted);
  }

  .hour {
    fill: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    /* In the arc's own box, like the time. */
    font-size: 6px;
    font-variant-numeric: tabular-nums;
  }

  .shine {
    flood-color: var(--ax-glow);
    flood-opacity: 0.7;
  }

  .time {
    fill: var(--ax-text);
    font-family: var(--ax-font-title);
    font-weight: 600;
    /* In the arc's own 100-unit box, so it grows with the arc rather than with the UI scale. */
    font-size: 15px;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }
</style>
