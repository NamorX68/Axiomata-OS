<!--
  The Orbit's time travel (`docs/plans/ui-ux-round.md` UX4, B2/G17): a quiet clock button at the corner that opens a
  slider over the vault's life — from its first note to today. Dragging shows the hive as it was then (notes not yet
  born stay unsettled cells, the ones born just then glow); ▶ lets the vault grow from its first note in a few
  seconds. Back at the right end it is today again, live.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import { formatDate } from "../core/format";
  import IconButton from "../ui/IconButton.svelte";

  interface Props {
    /** The vault's life: its first note's birth to now (`graph/hive.ts` `travelSpan`). */
    span: { from: number; to: number };
    /** The moment to show, or `null` for today. */
    onTravel: (at: number | null) => void;
  }

  let { span, onTravel }: Props = $props();

  /** How long ▶ takes to grow the vault from its first note to today. */
  const PLAY_MS = 8000;
  /** The slider's steps. */
  const STEPS = 1000;

  let open = $state(false);
  let step = $state(STEPS);
  let frame = 0;
  let playing = $state(false);

  const at = $derived(span.from + ((span.to - span.from) * step) / STEPS);
  const label = $derived(
    step >= STEPS
      ? "today"
      : formatDate(at, { day: "numeric", month: "short", year: "numeric" }),
  );

  function show(next: number): void {
    step = next;
    onTravel(next >= STEPS ? null : span.from + ((span.to - span.from) * next) / STEPS);
  }

  function stop(): void {
    cancelAnimationFrame(frame);
    frame = 0;
    playing = false;
  }

  function play(): void {
    stop();
    playing = true;
    const start = performance.now();
    const tick = (now: number): void => {
      const done = Math.min(1, (now - start) / PLAY_MS);
      show(Math.round(done * STEPS));
      frame = done < 1 ? requestAnimationFrame(tick) : 0;
      if (done >= 1) playing = false;
    };
    frame = requestAnimationFrame(tick);
  }

  function close(): void {
    stop();
    show(STEPS);
    open = false;
  }

  // Hidden (the Second Brain opened over the Orbit): back to today, or the hive would stay in the past while the
  // slider, mounted afresh, says "today".
  onDestroy(() => {
    stop();
    onTravel(null);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="travel"
  class:open
  data-no-drag
  onclick={(e) => e.stopPropagation()}
  onpointerdown={(e) => e.stopPropagation()}
>
  {#if open}
    <IconButton
      icon={playing ? "pause" : "play"}
      label={playing ? "Stop" : "Grow the vault"}
      size="sm"
      onclick={() => (playing ? stop() : play())}
    />
    <input
      type="range"
      min="0"
      max={STEPS}
      value={step}
      aria-label="Time travel"
      oninput={(e) => {
        stop();
        show(Number(e.currentTarget.value));
      }}
    />
    <span class="date">{label}</span>
    <IconButton icon="x" label="Back to today" size="sm" onclick={close} />
  {:else}
    <IconButton icon="history" label="Time travel: how the vault grew" onclick={() => (open = true)} />
  {/if}
</div>

<style>
  .travel {
    position: absolute;
    right: var(--ax-space-3);
    bottom: var(--ax-space-3);
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1);
    border-radius: var(--ax-radius-lg);
    opacity: 0.7;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }

  .travel:hover,
  .travel.open {
    opacity: 1;
  }

  .travel.open {
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    background: color-mix(in srgb, var(--ax-elev-2) 90%, transparent);
    box-shadow: var(--ax-shadow-panel);
  }

  input {
    width: calc(260px * var(--ax-ui-scale));
    accent-color: var(--ax-glow);
  }

  .date {
    min-width: calc(92px * var(--ax-ui-scale));
    font: var(--ax-type-caption);
    font-variant-numeric: tabular-nums;
    color: var(--ax-text-muted);
  }
</style>
