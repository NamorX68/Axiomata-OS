/**
 * The gliding cursor of the editor surface (`docs/plans/editor.md`, D8, G7;
 * `docs/plans/editor-look.md`, K8): a canvas layer that draws the cursor while
 * it moves to a new place — the farther, the longer it travels, and at the
 * `strong` setting with a glowing smear behind it that shrinks into the target
 * as it lands. At rest the canvas is empty and the surface shows its ordinary
 * blinking caret again.
 *
 * The motion itself is `editor/decorations.ts`'s pure `stepCursor`; this class
 * owns only what needs the browser: the animation frames and the canvas. Its
 * positions are in content pixels (the whole document, not the viewport), so
 * scrolling moves the drawing but never starts a glide.
 */

import {
  glideMotion,
  stepCursor,
  type CursorMotion,
  type GlideStrength,
  type MotionOptions,
} from "../editor/decorations";

/** Opacity of the smear where it meets the cursor (it fades to nothing at its tail). */
const SMEAR_ALPHA = 0.9;
/** A bar cursor is a hairline; its smear is at least this wide, centred on it, so a long jump reads as a streak. */
const SMEAR_MIN_WIDTH_PX = 14;
/** Blur radius of the accent glow around the cursor in flight, per strength. */
const GLOW_BLUR_PX: Record<GlideStrength, number> = { subtle: 12, strong: 34 };
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
  strength: GlideStrength;
  glow: boolean;
  /**
   * The cursor's shape (Vi, ED3 V8): a bar by default; a block is a cell wide
   * and see-through, an underline sits at the row's foot.
   */
  shape?: { width: number; height: number; alpha: number };
}

export class CursorGlide {
  private motion: CursorMotion | null = null;
  private target = { x: 0, y: 0 };
  private frame = 0;
  private lastFrame = 0;
  /** The look of the glide in flight, set by the latest `moveTo`. */
  private style: GlideStyle = { strength: "strong", glow: true };
  /** The glide in flight: set from the distance when it starts or its target changes. */
  private options: MotionOptions = glideMotion(0, "strong");

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
      this.motion = { x: target.x, y: target.y, tail: { ...target } };
      return;
    }
    this.style = style;
    // The farther the cursor has to go from where it is drawn now, the longer it travels (K8).
    this.options = glideMotion(Math.hypot(target.x - this.motion.x, target.y - this.motion.y), style.strength);
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
    const result = stepCursor(this.motion, this.target, now - this.lastFrame, this.options);
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

  /** Draws the cursor and its smear in the accent colour, at device resolution. */
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
    // Shown at exactly the scroller's size: a canvas stretched over the minimap or a
    // scrollbar would draw every glide that much too far right (and too low).
    const cssW = `${w}px`;
    const cssH = `${h}px`;
    if (canvas.style.width !== cssW) canvas.style.width = cssW;
    if (canvas.style.height !== cssH) canvas.style.height = cssH;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const styles = getComputedStyle(host.styles);
    const accent = styles.getPropertyValue("--ax-accent").trim() || "orange";
    const dx = host.textLeft - scroller.scrollLeft;
    const dy = -scroller.scrollTop;
    ctx.fillStyle = accent;
    const shape = this.style.shape ?? { width: CARET_WIDTH_PX, height: host.rowH, alpha: 1 };
    const foot = host.rowH - shape.height;
    const head = { x: this.motion.x + dx, y: this.motion.y + dy + foot };
    const tail = { x: this.motion.tail.x + dx, y: this.motion.tail.y + dy + foot };
    if (Math.hypot(head.x - tail.x, head.y - tail.y) > 1) {
      // One streak from the tail's box to the head's: their hull, fading towards the tail.
      const width = Math.max(shape.width, SMEAR_MIN_WIDTH_PX);
      const shift = (width - shape.width) / 2;
      const hull = rectHull({ x: tail.x - shift, y: tail.y }, { x: head.x - shift, y: head.y }, width, shape.height);
      const gradient = ctx.createLinearGradient(
        tail.x + shape.width / 2,
        tail.y + shape.height / 2,
        head.x + shape.width / 2,
        head.y + shape.height / 2,
      );
      gradient.addColorStop(0, "transparent");
      gradient.addColorStop(1, accent);
      ctx.globalAlpha = SMEAR_ALPHA * shape.alpha;
      ctx.fillStyle = gradient;
      ctx.beginPath();
      hull.forEach((p, i) => (i === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y)));
      ctx.closePath();
      ctx.fill();
      ctx.fillStyle = accent;
    }
    ctx.globalAlpha = shape.alpha;
    if (this.style.glow) {
      ctx.shadowColor = styles.getPropertyValue("--ax-editor-glow").trim() || accent;
      ctx.shadowBlur = GLOW_BLUR_PX[this.style.strength];
    }
    ctx.fillRect(this.motion.x + dx, this.motion.y + dy + foot, shape.width, shape.height);
    ctx.shadowBlur = 0;
    ctx.globalAlpha = 1;
  }
}

/**
 * The convex hull of two boxes of the same size (top-left corners `a` and `b`)
 * — the shape a box sweeps out moving from one to the other.
 */
export function rectHull(a: { x: number; y: number }, b: { x: number; y: number }, w: number, h: number) {
  const corners = [a, b].flatMap((p) => [
    { x: p.x, y: p.y },
    { x: p.x + w, y: p.y },
    { x: p.x + w, y: p.y + h },
    { x: p.x, y: p.y + h },
  ]);
  // Andrew's monotone chain.
  const pts = corners.sort((p, q) => p.x - q.x || p.y - q.y);
  const cross = (o: { x: number; y: number }, p: { x: number; y: number }, q: { x: number; y: number }) =>
    (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
  const half = (list: typeof pts) => {
    const out: typeof pts = [];
    for (const p of list) {
      while (out.length >= 2 && cross(out[out.length - 2], out[out.length - 1], p) <= 0) out.pop();
      out.push(p);
    }
    return out.slice(0, -1);
  };
  return [...half(pts), ...half([...pts].reverse())];
}
