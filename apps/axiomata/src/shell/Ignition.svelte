<!--
  The start ignition (`docs/plans/ui-ux-round.md` UX4, G10): at a cold start the hex logo draws itself, its core
  catches the glow, the name comes up — then the ground opens onto the app, about a second and a half in all. A click
  or a key skips it. Only at motion Full and while switched on under Appearance (`core/effects.ts`); otherwise the
  cover simply goes as soon as the saved settings are read.

  The cover is there from the first paint, so the app does not show itself, then vanish behind the ignition: it waits
  for the dashboard to be read (`dashboardLoaded`) to know whether to ignite at all.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { fade } from "svelte/transition";

  import { effects } from "../core/effects";
  import { animates, motion } from "../core/motion";
  import { dashboardLoaded } from "../core/persist";

  /** When the cover starts to open, from the start of the ignition (ms). */
  const OPEN_AT = 1250;
  /** How long it takes to open. */
  const OPEN_MS = 420;

  let phase = $state<"wait" | "ignite" | "done">("wait");
  let timer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (phase !== "wait" || !$dashboardLoaded) return;
    if ($effects.ignition && animates($motion)) {
      phase = "ignite";
      timer = setTimeout(() => (phase = "done"), OPEN_AT);
    } else {
      phase = "done";
    }
  });

  function skip(): void {
    if (timer) clearTimeout(timer);
    phase = "done";
  }

  onDestroy(() => {
    if (timer) clearTimeout(timer);
  });
</script>

<svelte:window onkeydowncapture={phase === "ignite" ? skip : undefined} />

{#if phase !== "done"}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="cover"
    class:ignite={phase === "ignite"}
    aria-hidden="true"
    onpointerdown={skip}
    out:fade={{ duration: OPEN_MS }}
  >
    {#if phase === "ignite"}
      <svg viewBox="0 0 100 100" class="logo">
        <path class="shell" d="M50 10 L84.6 30 L84.6 70 L50 90 L15.4 70 L15.4 30 Z" pathLength="1" />
        <path class="core" d="M50 31 L68.2 41.5 L68.2 62.5 L50 73 L31.8 62.5 L31.8 41.5 Z" />
      </svg>
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
    gap: var(--ax-space-3);
    background: var(--ax-bg);
  }

  /* While it fades out (Svelte makes an outroing element inert) the app beneath is already there to be used. */
  .cover:global([inert]) {
    pointer-events: none;
  }

  .logo {
    width: calc(96px * var(--ax-ui-scale));
    height: calc(96px * var(--ax-ui-scale));
    overflow: visible;
  }

  /* The outline draws itself… */
  .shell {
    fill: none;
    stroke: var(--ax-glow);
    stroke-width: 3;
    stroke-linejoin: round;
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: draw 560ms var(--ax-ease) forwards;
  }

  /* …then its core catches the glow. */
  .core {
    fill: var(--ax-glow);
    opacity: 0;
    transform-box: fill-box;
    transform-origin: center;
    animation: catch 420ms var(--ax-spring) 480ms forwards;
  }

  /* The glow on the whole mark, not on its SVG children: WebKit does not reliably draw CSS filters there. */
  .logo {
    filter: var(--ax-glow-drop);
  }

  .word {
    margin: 0;
    font: var(--ax-type-heading);
    letter-spacing: var(--ax-tracking-tight);
    color: var(--ax-text);
    opacity: 0;
    animation: rise 420ms var(--ax-ease) 700ms forwards;
  }

  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }

  @keyframes catch {
    from {
      opacity: 0;
      transform: scale(0.4);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
