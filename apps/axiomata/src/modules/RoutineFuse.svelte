<!--
  The fuse under the next routine (`docs/plans/ui-ux-round.md` UX3, "Zündschnur"): the time to its firing burns down
  like a fuse — ash behind the spark, the rest of the fuse ahead, a hex charge at the end. How far it has burnt is
  `core/fuse.ts`.

  At motion Full the spark throws a few embers (one animation frame loop, only while the fuse is on screen — the
  browser pauses it in a hidden window); at Calm and Off, or scrolled away, the spark holds still and the fuse is
  drawn again every few seconds.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";

  import { fuseProgress } from "../core/fuse";
  import { animates, motion, type MotionLevel } from "../core/motion";
  import { uiScale } from "../core/uiScale";

  interface Props {
    /** When the routine last fired (ms), or `null` if never. */
    lastFired: number | null;
    /** When it fires next (ms). */
    nextFire: number;
  }

  let { lastFired, nextFire }: Props = $props();

  /** How often a still fuse is drawn again: it moves by minutes, not frames. */
  const STILL_REDRAW_MS = 5000;
  /** The fuse's height in unscaled pixels. */
  const HEIGHT = 18;

  let canvas = $state<HTMLCanvasElement | null>(null);

  interface Ember {
    x: number;
    y: number;
    vx: number;
    vy: number;
    life: number;
  }

  onMount(() => {
    const el = canvas;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;
    let embers: Ember[] = [];
    let raf = 0;
    let timer: ReturnType<typeof setInterval> | null = null;
    let last = performance.now();

    const size = (): void => {
      const dpr = window.devicePixelRatio || 1;
      el.width = Math.round(el.clientWidth * dpr);
      el.height = Math.round(HEIGHT * get(uiScale) * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    const hex = (cx: number, cy: number, r: number): void => {
      ctx.beginPath();
      for (let i = 0; i < 6; i++) {
        const a = (Math.PI / 3) * i - Math.PI / 2;
        ctx.lineTo(cx + r * Math.cos(a), cy + r * Math.sin(a));
      }
      ctx.closePath();
    };

    const draw = (now: number, withEmbers: boolean): void => {
      const style = getComputedStyle(el);
      const color = (name: string): string => style.getPropertyValue(name).trim();
      const scale = get(uiScale);
      const w = el.clientWidth;
      const h = HEIGHT * scale;
      const y = h / 2;
      const r = 5 * scale;
      const x0 = 2 * scale;
      const x1 = w - r * 2 - 4 * scale;
      const t = fuseProgress(lastFired, nextFire, Date.now());
      const bx = x0 + (x1 - x0) * t;
      ctx.clearRect(0, 0, w, h);

      // Ash behind the spark, the fuse ahead with a slight twist.
      ctx.lineWidth = 1.5 * scale;
      ctx.setLineDash([2 * scale, 4 * scale]);
      ctx.strokeStyle = color("--ax-ash");
      ctx.beginPath();
      ctx.moveTo(x0, y);
      ctx.lineTo(bx, y);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.strokeStyle = color("--ax-text-muted");
      ctx.beginPath();
      for (let x = bx; x <= x1; x += 3) ctx.lineTo(x, y + Math.sin(x * 0.15) * scale);
      ctx.stroke();

      // The charge it burns towards.
      hex(x1 + r + 3 * scale, y, r);
      ctx.fillStyle = color("--ax-elev-3");
      ctx.strokeStyle = color("--ax-border-strong");
      ctx.fill();
      ctx.stroke();

      // The spark: the one glowing thing here. A blurred shadow rather than a gradient to "transparent", which would
      // fade through transparent black and leave a dark fringe.
      const glowColor = color("--ax-glow");
      ctx.save();
      ctx.shadowColor = glowColor;
      ctx.shadowBlur = 8 * scale;
      ctx.fillStyle = glowColor;
      ctx.beginPath();
      ctx.arc(bx, y, 2.2 * scale, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();

      if (!withEmbers) return;
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      if (Math.random() < 0.5) {
        embers.push({ x: bx, y, vx: (Math.random() - 0.6) * 40, vy: -Math.random() * 40, life: 1 });
      }
      embers = embers.filter((e) => e.life > 0);
      ctx.fillStyle = glowColor;
      for (const e of embers) {
        e.x += e.vx * dt;
        e.y += e.vy * dt;
        e.vy += 120 * dt;
        e.life -= dt * 2.4;
        ctx.globalAlpha = Math.max(0, e.life);
        ctx.fillRect(e.x, e.y, 1.5 * scale, 1.5 * scale);
      }
      ctx.globalAlpha = 1;
    };

    const stop = (): void => {
      cancelAnimationFrame(raf);
      if (timer !== null) clearInterval(timer);
      timer = null;
    };

    let level: MotionLevel = "full";
    let onScreen = true;

    // The ember loop runs only at Full and while the fuse is on screen; otherwise it is drawn again now and then.
    const start = (): void => {
      stop();
      if (animates(level) && onScreen) {
        const frame = (now: number): void => {
          draw(now, true);
          raf = requestAnimationFrame(frame);
        };
        raf = requestAnimationFrame(frame);
      } else {
        embers = [];
        draw(performance.now(), false);
        timer = setInterval(() => draw(performance.now(), false), STILL_REDRAW_MS);
      }
    };

    const resize = (): void => {
      size();
      draw(performance.now(), false);
    };
    size();
    const observer = new ResizeObserver(resize);
    observer.observe(el);
    const visibility = new IntersectionObserver((entries) => {
      onScreen = entries.some((e) => e.isIntersecting);
      start();
    });
    visibility.observe(el);
    // A move to a display of another density changes no size, only the pixel ratio: the canvas must follow it.
    const density = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    density.addEventListener("change", resize);
    const unsubscribe = motion.subscribe((next) => {
      level = next;
      start();
    });
    return () => {
      stop();
      unsubscribe();
      observer.disconnect();
      visibility.disconnect();
      density.removeEventListener("change", resize);
    };
  });
</script>

<canvas class="fuse" bind:this={canvas} aria-hidden="true"></canvas>

<style>
  .fuse {
    display: block;
    width: 100%;
    height: calc(18px * var(--ax-ui-scale));
  }
</style>
