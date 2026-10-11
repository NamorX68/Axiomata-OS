<!--
  Clock — the optional widget right of the mini-month in the `calendar`
  module (toggled by `config.showClock`, styled by `config.clockStyle`):
  the digital time and date, or the day arc (`DayArc.svelte`, G11), which
  replaced the analogue face. `size` (px) is the mini-month's rendered height
  so the two line up. One 20 s timer (no per-second repaint) — `HH:mm` can lag
  the wall clock by up to that, which is fine for a dashboard clock.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import type { CalendarEvent } from "../core/calendar";
  import DayArc from "./DayArc.svelte";

  interface Props {
    style?: "digital" | "arc";
    size: number;
    /** The calendar's events, for the arc. */
    events?: readonly CalendarEvent[];
  }

  let { style = "digital", size, events = [] }: Props = $props();

  let now = $state(new Date());
  const timer = setInterval(() => (now = new Date()), 20_000);
  onDestroy(() => clearInterval(timer));

  const hh = $derived(String(now.getHours()).padStart(2, "0"));
  const mm = $derived(String(now.getMinutes()).padStart(2, "0"));
  const dateLine = $derived(
    now.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", year: "numeric" }),
  );
</script>

{#if style === "arc"}
  <DayArc {events} {size} {now} />
{:else}
  <div class="clock digital" style="height: {size}px">
    <span class="time" style="font-size: {Math.min(52, Math.round(size * 0.32))}px">{hh}:{mm}</span>
    <span class="date" style="font-size: {Math.max(10, Math.min(15, Math.round(size * 0.11)))}px">{dateLine}</span>
  </div>
{/if}

<style>
  .clock {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
  }
  .clock.digital {
    flex-direction: column;
    gap: var(--ax-space-1);
    padding: 0 var(--ax-space-1);
  }
  /* Figures in the title face, not code: the mono face is for code (U1 §3). */
  .time {
    font-family: var(--ax-font-title);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
    line-height: 1;
    color: var(--ax-text);
  }
  .date {
    color: var(--ax-text-muted);
    white-space: nowrap;
    line-height: 1;
  }
</style>
