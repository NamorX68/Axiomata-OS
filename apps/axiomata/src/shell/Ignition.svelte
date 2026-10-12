<!--
  The start ignition (`docs/plans/ui-ux-round.md` UX4, G10): at a cold start six embers fly in to the corners of the hex
  logo, its outline draws itself between them, its core catches with a flare and a shock wave, the name pulls
  together — then the mark opens towards the viewer onto the app, about two and a half seconds in all (the owner
  wanted it a little longer and bolder than the first cut). A click or a key skips it. Only at motion Full and while
  switched on under Appearance (`core/effects.ts`); otherwise the cover simply goes as soon as the saved settings are
  read.

  The cover is there from the first paint, so the app does not show itself, then vanish behind the ignition: it waits
  for the dashboard to be read (`dashboardLoaded`) to know whether to ignite at all.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { fade } from "svelte/transition";

  import { effects } from "../core/effects";
  import { animates, motion } from "../core/motion";
  import { dashboardLoaded } from "../core/persist";

  /** When the mark starts to open onto the app, from the start of the ignition (ms). */
  const OPEN_AT = 2000;
  /** How long the opening takes; the CSS reads it as `--open-ms`. */
  const OPEN_MS = 520;
  /** The hex's corners in the logo's 100-unit box: where the embers land. */
  const CORNERS: readonly [number, number][] = [
    [50, 10],
    [84.6, 30],
    [84.6, 70],
    [50, 90],
    [15.4, 70],
    [15.4, 30],
  ];
  /** How far out an ember starts, as a multiple of its corner's distance from the centre. */
  const EMBER_FROM = 3.2;

  let phase = $state<"wait" | "ignite" | "open" | "done">("wait");
  let timers: ReturnType<typeof setTimeout>[] = [];
  /** The mark has opened onto the app (the cover is already transparent): it then goes without a second fade. */
  let opened = $state(false);

  $effect(() => {
    if (phase !== "wait" || !$dashboardLoaded) return;
    if ($effects.ignition && animates($motion)) {
      phase = "ignite";
      timers = [
        setTimeout(() => {
          opened = true;
          phase = "open";
        }, OPEN_AT),
        setTimeout(() => (phase = "done"), OPEN_AT + OPEN_MS),
      ];
    } else {
      phase = "done";
    }
  });

  function clearTimers(): void {
    timers.forEach(clearTimeout);
    timers = [];
  }

  function skip(): void {
    clearTimers();
    phase = "done";
  }

  onDestroy(clearTimers);
</script>

<svelte:window onkeydowncapture={phase === "ignite" ? skip : undefined} />

{#if phase !== "done"}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="cover"
    class:ignite={phase !== "wait"}
    class:open={phase === "open"}
    aria-hidden="true"
    onpointerdown={skip}
    style:--open-ms="{OPEN_MS}ms"
    out:fade={{ duration: opened ? 0 : OPEN_MS }}
  >
    {#if phase !== "wait"}
      <div class="mark">
        <div class="flare"></div>
        <svg viewBox="0 0 100 100" class="logo">
          <path class="wave" d="M50 10 L84.6 30 L84.6 70 L50 90 L15.4 70 L15.4 30 Z" />
          {#each CORNERS as [x, y], i (i)}
            <circle
              class="ember"
              cx={x}
              cy={y}
              r="2.6"
              style:--dx="{(x - 50) * (EMBER_FROM - 1)}px"
              style:--dy="{(y - 50) * (EMBER_FROM - 1)}px"
              style:--delay="{i * 45}ms"
            />
          {/each}
          <path class="shell" d="M50 10 L84.6 30 L84.6 70 L50 90 L15.4 70 L15.4 30 Z" pathLength="1" />
          <path class="core" d="M50 31 L68.2 41.5 L68.2 62.5 L50 73 L31.8 62.5 L31.8 41.5 Z" />
        </svg>
      </div>
      <p class="word">Axiomata</p>
    {/if}
  </div>
{/if}

<style>
  .cover {
    position: fixed;
    inset: 0;
    z-index: var(--ax-z-toast);
    display: grid;
    place-content: center;
    justify-items: center;
    gap: var(--ax-space-4);
    background: var(--ax-bg);
  }

  /* While it opens or fades out (Svelte makes an outroing element inert) the app beneath is already there to be used. */
  .cover.open,
  .cover:global([inert]) {
    pointer-events: none;
  }

  /* The opening: the mark comes towards the viewer and the ground goes, showing the app. */
  .cover.open {
    opacity: 0;
    transition: opacity var(--open-ms) var(--ax-ease);
  }

  .cover.open .mark {
    transform: scale(1.8);
    opacity: 0;
    transition:
      transform var(--open-ms) var(--ax-ease),
      opacity calc(var(--open-ms) * 0.8) var(--ax-ease);
  }

  .mark {
    position: relative;
    display: grid;
    place-items: center;
  }

  .logo {
    position: relative;
    width: calc(112px * var(--ax-ui-scale));
    height: calc(112px * var(--ax-ui-scale));
    overflow: visible;
    /* The glow on the whole mark, not on its SVG children: WebKit does not reliably draw CSS filters there. */
    filter: var(--ax-glow-drop);
  }

  /* The light the core throws when it catches: a flash, then a soft afterglow. */
  .flare {
    position: absolute;
    width: calc(340px * var(--ax-ui-scale));
    height: calc(340px * var(--ax-ui-scale));
    border-radius: 50%;
    background: radial-gradient(circle, var(--ax-glow-strong) 0%, var(--ax-glow-soft) 35%, transparent 70%);
    opacity: 0;
    transform: scale(0.3);
    animation: flare 900ms var(--ax-ease) 1080ms forwards;
  }

  /* Six embers fly in to the corners… */
  .ember {
    fill: var(--ax-glow);
    opacity: 0;
    transform: translate(var(--dx), var(--dy));
    animation: fly-in 560ms var(--ax-ease) var(--delay) forwards;
  }

  /* …the outline draws itself between them… */
  .shell {
    fill: none;
    stroke: var(--ax-glow);
    stroke-width: 3;
    stroke-linejoin: round;
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: draw 720ms var(--ax-ease) 420ms forwards;
  }

  /* …its core catches the glow… */
  .core {
    fill: var(--ax-glow);
    opacity: 0;
    transform-box: fill-box;
    transform-origin: center;
    animation: catch 460ms var(--ax-spring) 1060ms forwards;
  }

  /* …and the catch sends a wave out. */
  .wave {
    fill: none;
    stroke: var(--ax-glow);
    stroke-width: 1.5;
    stroke-linejoin: round;
    opacity: 0;
    transform-box: fill-box;
    transform-origin: center;
    animation: wave 760ms var(--ax-ease) 1100ms forwards;
  }

  .word {
    margin: 0;
    font: var(--ax-type-heading);
    letter-spacing: 0.4em;
    color: var(--ax-text);
    opacity: 0;
    animation: pull-together 560ms var(--ax-ease) 1260ms forwards;
  }

  @keyframes fly-in {
    0% {
      opacity: 0;
      transform: translate(var(--dx), var(--dy));
    }
    30% {
      opacity: 1;
    }
    80% {
      opacity: 1;
      transform: none;
    }
    100% {
      opacity: 0.9;
      transform: none;
    }
  }

  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }

  @keyframes catch {
    from {
      opacity: 0;
      transform: scale(0.3);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  @keyframes wave {
    0% {
      opacity: 0.9;
      transform: scale(1);
    }
    100% {
      opacity: 0;
      transform: scale(2.6);
    }
  }

  @keyframes flare {
    0% {
      opacity: 0;
      transform: scale(0.3);
    }
    25% {
      opacity: 1;
      transform: scale(1);
    }
    100% {
      opacity: 0.45;
      transform: scale(0.85);
    }
  }

  @keyframes pull-together {
    from {
      opacity: 0;
      letter-spacing: 0.4em;
      transform: translateY(calc(6px * var(--ax-ui-scale)));
    }
    to {
      opacity: 1;
      letter-spacing: var(--ax-tracking-tight);
      transform: none;
    }
  }
</style>
