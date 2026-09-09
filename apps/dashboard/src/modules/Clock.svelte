<!--
  Clock — the optional widget right of the mini-month in the `calendar`
  module (toggled by `config.showClock`, styled by `config.clockStyle`).
  Purely presentational: `size` (px) is the mini-month's rendered height so
  the two line up. One 20 s timer (no per-second repaint) — `HH:mm` can lag
  the wall clock by up to that, which is fine for a dashboard clock.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  let { style = "digital", size }: { style?: "digital" | "analog"; size: number } = $props();

  let now = $state(new Date());
  const timer = setInterval(() => (now = new Date()), 20_000);
  onDestroy(() => clearInterval(timer));

  const hh = $derived(String(now.getHours()).padStart(2, "0"));
  const mm = $derived(String(now.getMinutes()).padStart(2, "0"));
  const dateLine = $derived(
    now.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", year: "numeric" }),
  );

  // Hand angles in degrees, 12 o'clock = 0.
  const minuteAngle = $derived(now.getMinutes() * 6 + now.getSeconds() * 0.1);
  const hourAngle = $derived((now.getHours() % 12) * 30 + now.getMinutes() * 0.5);

  const ticks = Array.from({ length: 12 }, (_, i) => i);
</script>

{#if style === "analog"}
  <div class="clock analog" style="width: {size}px; height: {size}px" role="img" aria-label={`${hh}:${mm}`}>
    <svg viewBox="0 0 100 100" width={size} height={size}>
      <circle class="face" cx="50" cy="50" r="47" />
      {#each ticks as i (i)}
        <line class="tick" x1="50" y1="6" x2="50" y2="12" transform="rotate({i * 30} 50 50)" />
      {/each}
      <line class="hand hour" x1="50" y1="54" x2="50" y2="30" transform="rotate({hourAngle} 50 50)" />
      <line class="hand minute" x1="50" y1="56" x2="50" y2="16" transform="rotate({minuteAngle} 50 50)" />
      <circle class="pin" cx="50" cy="50" r="2.6" />
    </svg>
  </div>
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
  .time {
    font-family: var(--ax-font-mono);
    font-weight: 700;
    letter-spacing: 0.02em;
    line-height: 1;
    color: var(--ax-text);
    /* Knock the near-white down a touch so the large digits don't glare;
       still clearly brighter than the muted date line below. */
    opacity: 0.78;
  }
  .date {
    color: var(--ax-text-muted);
    white-space: nowrap;
    line-height: 1;
  }

  .clock.analog {
    max-width: 100%;
  }
  .clock.analog svg {
    max-width: 100%;
    height: auto;
  }
  .face {
    fill: var(--ax-surface-2);
    stroke: var(--ax-border);
    stroke-width: 1.5;
  }
  .tick {
    stroke: var(--ax-text-muted);
    stroke-width: 2;
    stroke-linecap: round;
  }
  .hand {
    stroke-linecap: round;
  }
  .hand.hour {
    stroke: var(--ax-text);
    stroke-width: 4.5;
  }
  .hand.minute {
    stroke: var(--ax-accent);
    stroke-width: 3;
  }
  .pin {
    fill: var(--ax-accent);
  }
</style>
