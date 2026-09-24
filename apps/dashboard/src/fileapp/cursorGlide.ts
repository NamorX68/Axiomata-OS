/**
 * The gliding cursor of the editor surface (`docs/plans/editor.md`, D8, G7):
 * a canvas layer that draws the cursor — and, if asked, a fading trail — while
 * it moves to a new place. At rest the canvas is empty and the surface shows its
 * ordinary blinking caret again.
 *
 * The motion itself is `editor/decorations.ts`'s pure `stepCursor`; this class
 * owns only what needs the browser: the animation frames and the canvas. Its
 * positions are in content pixels (the whole document, not the viewport), so
 * scrolling moves the drawing but never starts a glide.
 */

import { stepCursor, type CursorMotion } from "../editor/decorations";

/** How long a glide takes. */
const GLIDE_MS = 110;
/** Peak opacity of the oldest-to-newest trail marks. */
const TRAIL_ALPHA = 0.35;
/** Blur radius of the accent glow around the cursor. */
const GLOW_BLUR_PX = 8;
/** Width of the drawn cursor bar. */
const CARET_WIDTH_PX = 2;

/** What a frame needs to know about the surface it draws over. */
export interface GlideHost {
  canvas: HTMLCanvasElement;
  /** The scroller the canvas covers (its size and scroll offsets). */
  scroller: HTMLElement;
  /** The element the `--ax-*` colours are read from. */
  styles: HTMLElement;
  /** Where the text starts in the scroller, and how tall a row is. */
  textLeft: number;
  rowH: number;
}

export interface GlideStyle {
  trail: boolean;
  glow: boolean;
}

export class CursorGlide {
  private motion: CursorMotion | null = null;
  private target = { x: 0, y: 0 };
  private frame = 0;
  private lastFrame = 0;
  /** The look of the glide in flight, set by the latest `moveTo`. */
  private style: GlideStyle = { trail: true, glow: true };

  /**
   * @param host Read on every frame, so it always reflects the surface now.
   * @param onGliding Told when a glide starts (true) and when it has arrived (false).
   */
  constructor(
    private readonly host: () => GlideHost | null,
    private readonly onGliding: (gliding: boolean) => void,
  ) {}

  /**
   * Moves the cursor to `target` (content pixels). With `style` `null` — the
   * glide is off — or on the very first call it jumps there without a frame.
   */
  moveTo(target: { x: number; y: number }, style: GlideStyle | null): void {
    this.target = target;
    if (!style || !this.motion) {
      this.motion = { x: target.x, y: target.y, trail: [] };
      return;
    }
    this.style = style;
    if (this.frame === 0 && (this.motion.x !== target.x || this.motion.y !== target.y)) {
      this.lastFrame = performance.now();
      this.onGliding(true);
      this.frame = requestAnimationFrame((now) => this.step(now));
    }
  }

  dispose(): void {
    if (this.frame) cancelAnimationFrame(this.frame);
    this.frame = 0;
  }

  private step(now: number): void {
    if (!this.motion) return;
    const result = stepCursor(this.motion, this.target, now - this.lastFrame, {
      durationMs: GLIDE_MS,
      trail: this.style.trail,
    });
    this.lastFrame = now;
    this.motion = result.motion;
    const host = this.host();
    if (host) this.draw(host);
    if (result.done) {
      this.frame = 0;
      this.onGliding(false);
      host?.canvas.getContext("2d")?.clearRect(0, 0, host.canvas.width, host.canvas.height);
    } else {
      this.frame = requestAnimationFrame((t) => this.step(t));
    }
  }

  /** Draws the cursor and its fading trail in the accent colour, at device resolution. */
  private draw(host: GlideHost): void {
    const { canvas, scroller } = host;
    const ctx = canvas.getContext("2d");
    if (!ctx || !this.motion) return;
    const dpr = window.devicePixelRatio || 1;
    const w = scroller.clientWidth;
    const h = scroller.clientHeight;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const styles = getComputedStyle(host.styles);
    const accent = styles.getPropertyValue("--ax-accent").trim() || "orange";
    const dx = host.textLeft - scroller.scrollLeft;
    const dy = -scroller.scrollTop;
    ctx.fillStyle = accent;
    const trail = this.motion.trail;
    trail.forEach((p, i) => {
      ctx.globalAlpha = ((i + 1) / (trail.length + 1)) * TRAIL_ALPHA;
      ctx.fillRect(p.x + dx, p.y + dy, CARET_WIDTH_PX, host.rowH);
    });
    ctx.globalAlpha = 1;
    if (this.style.glow) {
      ctx.shadowColor = styles.getPropertyValue("--ax-editor-glow").trim() || accent;
      ctx.shadowBlur = GLOW_BLUR_PX;
    }
    ctx.fillRect(this.motion.x + dx, this.motion.y + dy, CARET_WIDTH_PX, host.rowH);
    ctx.shadowBlur = 0;
  }
}
