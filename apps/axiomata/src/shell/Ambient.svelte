<!--
  The ambient light over the ground (`docs/plans/ui-ux-round.md` UX3, G10), below everything else: a very faint light
  that follows the time of day (`core/daylight.ts`), and the hex lines lighting up around the pointer — the hexagon
  being the system's shape, they show even in a theme whose own texture is another one. Each switchable under
  Appearance (`core/effects.ts`); light themes get half of it (`--ax-ambient`).

  The pointer light is a small disc moved with a transform, its hex mask shifted back by the same amount so the lit
  lines stay on the ground's own: a move repaints that disc only, not the window, and goes past Svelte's reactivity
  (CSS variables written once per animation frame). Nothing listens while the light is switched off.
-->
<script lang="ts">
  import { get } from "svelte/store";

  import { dayLightAt, type DayLight } from "../core/daylight";
  import { effects } from "../core/effects";
  import { uiScale } from "../core/uiScale";

  /** How often the day's light is placed again; it moves by the hour. */
  const DAY_REFRESH_MS = 5 * 60 * 1000;
  /** The strongest the day's light gets, in percent of its colour: a tint to notice only when it is gone. */
  const DAY_ALPHA = 7;
  /** The pointer light's diameter in unscaled pixels (the CSS draws it times the UI scale, too). */
  const POINTER_DIAMETER = 400;

  let pointerEl = $state<HTMLElement | null>(null);
  let light = $state<DayLight>(lightNow());

  function lightNow(): DayLight {
    const d = new Date();
    return dayLightAt(d.getHours() + d.getMinutes() / 60);
  }

  /** The day's colour: warm low in the sky, neutral high up, cool at night. */
  const dayColor = $derived(
    light.night
      ? "var(--ax-daylight-cool)"
      : `color-mix(in srgb, var(--ax-daylight-warm) ${Math.round(light.warmth * 100)}%, var(--ax-daylight-neutral))`,
  );

  $effect(() => {
    if (!$effects.dayLight) return;
    const timer = setInterval(() => (light = lightNow()), DAY_REFRESH_MS);
    return () => clearInterval(timer);
  });

  $effect(() => {
    const el = pointerEl;
    if (!$effects.pointerLight || !el) return;
    let frame = 0;
    let x = 0;
    let y = 0;
    const paint = (): void => {
      frame = 0;
      const half = (POINTER_DIAMETER * get(uiScale)) / 2;
      el.style.setProperty("--ox", `${x - half}px`);
      el.style.setProperty("--oy", `${y - half}px`);
    };
    const onMove = (e: PointerEvent): void => {
      x = e.clientX;
      y = e.clientY;
      el.classList.add("on");
      if (frame === 0) frame = requestAnimationFrame(paint);
    };
    // Leaving the window: `pointerleave` on the root is unreliable in WebKit, so the document's mouse leave and a lost
    // focus both put the light out.
    const off = (): void => el.classList.remove("on");
    window.addEventListener("pointermove", onMove, { passive: true });
    document.addEventListener("mouseleave", off);
    window.addEventListener("blur", off);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("pointermove", onMove);
      document.removeEventListener("mouseleave", off);
      window.removeEventListener("blur", off);
    };
  });
</script>

{#if $effects.dayLight}
  <div
    class="day"
    aria-hidden="true"
    style:--dx="{light.x}%"
    style:--dy="{light.y}%"
    style:--day-color={dayColor}
    style:--day-alpha="{(DAY_ALPHA * light.strength).toFixed(2)}%"
  ></div>
{/if}
{#if $effects.pointerLight}
  <div class="pointer" aria-hidden="true" bind:this={pointerEl}></div>
{/if}

<style>
  .day,
  .pointer {
    position: fixed;
    pointer-events: none;
  }

  .day {
    inset: 0;
    background: radial-gradient(
      ellipse 75% 65% at var(--dx) var(--dy),
      color-mix(in srgb, var(--day-color) calc(var(--day-alpha) * var(--ax-ambient)), transparent),
      transparent 75%
    );
  }

  /* A disc of soft light, moved to the pointer; the mask, shifted back by the same offset, lets it fall only on the
     hex lines of the ground. */
  .pointer {
    --ox: -9999px;
    --oy: -9999px;
    top: 0;
    left: 0;
    width: calc(400px * var(--ax-ui-scale));
    height: calc(400px * var(--ax-ui-scale));
    transform: translate(var(--ox), var(--oy));
    background: radial-gradient(
      circle closest-side,
      /* 35 %: at 60 the lit lines drew the eye away from the tiles (owner, 2026-10-12). */
      color-mix(in srgb, var(--ax-glow) calc(35% * var(--ax-ambient)), transparent),
      transparent
    );
    -webkit-mask-image: var(--ax-hex-mask-url);
    mask-image: var(--ax-hex-mask-url);
    -webkit-mask-size: var(--ax-hex-mask-size);
    mask-size: var(--ax-hex-mask-size);
    -webkit-mask-position: calc(-1 * var(--ox)) calc(-1 * var(--oy));
    mask-position: calc(-1 * var(--ox)) calc(-1 * var(--oy));
    opacity: 0;
    transition: opacity var(--ax-dur-med) var(--ax-ease);
    will-change: transform;
  }

  .pointer:global(.on) {
    opacity: 1;
  }
</style>
