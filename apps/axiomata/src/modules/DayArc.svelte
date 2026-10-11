<!--
  The calendar's day arc (`docs/plans/ui-ux-round.md` UX3, G11), in place of the analogue clock: the waking day as a
  half circle, the part already gone a shade darker, today's events as segments (the one going on glows), "now" as a
  hexagon on the arc and the time in the middle. Geometry in `core/dayArc.ts`.
-->
<script lang="ts">
  import type { CalendarEvent } from "../core/calendar";
  import { arcFraction, arcPath, arcPoint, arcSegments } from "../core/dayArc";

  interface Props {
    /** The events the calendar shows; the arc takes today's timed ones. */
    events: readonly CalendarEvent[];
    /** Rendered height in px, the mini-month's, so the two line up. */
    size: number;
    /** The clock's time, from the parent's timer. */
    now: Date;
  }

  let { events, size, now }: Props = $props();

  const at = $derived(now.getTime());
  const nowAt = $derived(arcFraction(at));
  const here = $derived(arcPoint(nowAt));
  const segments = $derived(arcSegments(events, at));
  const time = $derived(now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));

  /** A small hexagon around a point, as SVG points. */
  function hex(cx: number, cy: number, r: number): string {
    return Array.from({ length: 6 }, (_, i) => {
      const a = (Math.PI / 3) * i - Math.PI / 2;
      return `${(cx + r * Math.cos(a)).toFixed(2)},${(cy + r * Math.sin(a)).toFixed(2)}`;
    }).join(" ");
  }
</script>

<div class="clock arc" style:height="{size}px" style:width="{Math.round(size * 1.6)}px">
  <svg viewBox="0 0 100 62" role="img" aria-label="{time}, {segments.length} events today">
    <path class="track" d={arcPath(0, 1)} />
    <path class="gone" d={arcPath(0, nowAt)} />
    {#each segments as s (s.id)}
      <path class="event" class:live={s.live} d={arcPath(s.from, s.to)}><title>{s.title}</title></path>
    {/each}
    <polygon class="now" points={hex(here.x, here.y, 3.2)} />
    <text class="time" x="50" y="46" text-anchor="middle">{time}</text>
  </svg>
</div>

<style>
  .clock.arc {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    max-width: 100%;
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
    filter: var(--ax-glow-drop);
    animation: var(--ax-anim-breathe);
  }

  .now {
    fill: var(--ax-glow);
    filter: var(--ax-glow-drop);
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
