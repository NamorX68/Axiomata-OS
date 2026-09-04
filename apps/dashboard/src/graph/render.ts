/**
 * Canvas-2D renderer for the graph: area ring segments, edges, nodes with a
 * soft glow and a slow twinkle, optional labels, a slow spin, hover hit-test,
 * and a view transform (pan / zoom) the full-screen view drives. Draws with
 * the device pixel ratio; call `resize()` when the canvas box changes.
 */

import { glyphForArea, type GraphModel, type GraphNode } from "./model";

export type RenderMode = "rings" | "orbit" | "hex";

export interface RenderOptions {
  /** `rings`/`hex` = the Second Brain view (dots vs. honeycomb cells for
   *  files); `orbit` = the dashboard centre. */
  mode?: RenderMode;
  /** Radians per second of ring rotation. */
  spin: number;
  /** Draw skill / routine / area labels. */
  labels: boolean;
  /** Draw file titles (full view only). */
  fileLabels: boolean;
  /** Fraction of the shorter canvas side used as the outer radius. */
  fit: number;
}

export interface View {
  /** Pan offset in CSS px from the canvas centre. */
  x: number;
  y: number;
  /** Zoom multiplier. */
  zoom: number;
}

const TWO_PI = Math.PI * 2;

/**
 * Small vector glyph inside a non-file node, drawn in the node's contrast
 * colour. `glyph` is either a structural id — "hub" (hexagon), "skill"
 * (bolt), "routine" (clock), "folder" (the generic area default) — or one
 * of the per-area icons from `model.glyphForArea` (e.g. "book", "code",
 * "briefcase"). Unknown ids fall back to nothing drawn (just the ring).
 */
export function drawGlyph(
  ctx: CanvasRenderingContext2D,
  glyph: string,
  x: number,
  y: number,
  r: number,
  color: string,
): void {
  const s = r * 0.62;
  ctx.save();
  ctx.translate(x, y);
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = Math.max(1, r * 0.16);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.globalAlpha = 0.95;
  ctx.beginPath();
  switch (glyph) {
    case "hub":
      for (let i = 0; i < 6; i++) {
        const a = -Math.PI / 2 + (i * Math.PI) / 3;
        const px = Math.cos(a) * s;
        const py = Math.sin(a) * s;
        if (i === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      }
      ctx.closePath();
      ctx.stroke();
      break;
    case "folder":
      ctx.moveTo(-s, -s * 0.55);
      ctx.lineTo(-s * 0.3, -s * 0.55);
      ctx.lineTo(-s * 0.05, -s * 0.25);
      ctx.lineTo(s, -s * 0.25);
      ctx.lineTo(s, s * 0.6);
      ctx.lineTo(-s, s * 0.6);
      ctx.closePath();
      ctx.stroke();
      break;
    case "skill":
      ctx.moveTo(s * 0.25, -s);
      ctx.lineTo(-s * 0.55, s * 0.1);
      ctx.lineTo(s * 0.05, s * 0.1);
      ctx.lineTo(-s * 0.25, s);
      ctx.lineTo(s * 0.55, -s * 0.1);
      ctx.lineTo(-s * 0.05, -s * 0.1);
      ctx.closePath();
      ctx.fill();
      break;
    case "routine":
      ctx.arc(0, 0, s, 0, TWO_PI);
      ctx.moveTo(0, -s * 0.55);
      ctx.lineTo(0, 0);
      ctx.lineTo(s * 0.45, s * 0.25);
      ctx.stroke();
      break;
    case "code": // </> — Entwicklung / Rust / BlockOS
      ctx.moveTo(-s * 0.15, -s * 0.55);
      ctx.lineTo(-s * 0.8, 0);
      ctx.lineTo(-s * 0.15, s * 0.55);
      ctx.moveTo(s * 0.15, -s * 0.55);
      ctx.lineTo(s * 0.8, 0);
      ctx.lineTo(s * 0.15, s * 0.55);
      ctx.stroke();
      break;
    case "chip": // KI
      ctx.roundRect(-s * 0.5, -s * 0.5, s, s, s * 0.12);
      ctx.stroke();
      for (const o of [-0.55, 0.55]) {
        ctx.beginPath();
        ctx.moveTo(o * s * 0.6, -s * 0.5);
        ctx.lineTo(o * s * 0.6, -s * 0.75);
        ctx.moveTo(o * s * 0.6, s * 0.5);
        ctx.lineTo(o * s * 0.6, s * 0.75);
        ctx.stroke();
      }
      break;
    case "book": // Learning
      ctx.moveTo(0, -s * 0.5);
      ctx.quadraticCurveTo(-s * 0.95, -s * 0.7, -s * 0.95, s * 0.05);
      ctx.quadraticCurveTo(-s * 0.95, s * 0.6, 0, s * 0.4);
      ctx.moveTo(0, -s * 0.5);
      ctx.quadraticCurveTo(s * 0.95, -s * 0.7, s * 0.95, s * 0.05);
      ctx.quadraticCurveTo(s * 0.95, s * 0.6, 0, s * 0.4);
      ctx.moveTo(0, -s * 0.5);
      ctx.lineTo(0, s * 0.4);
      ctx.stroke();
      break;
    case "briefcase": // Arbeit
      ctx.roundRect(-s * 0.9, -s * 0.25, s * 1.8, s * 0.95, s * 0.15);
      ctx.moveTo(-s * 0.35, -s * 0.25);
      ctx.lineTo(-s * 0.35, -s * 0.55);
      ctx.lineTo(s * 0.35, -s * 0.55);
      ctx.lineTo(s * 0.35, -s * 0.25);
      ctx.stroke();
      break;
    case "camera": // Fotografie
      ctx.roundRect(-s * 0.9, -s * 0.35, s * 1.8, s * 0.95, s * 0.15);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(0, s * 0.12, s * 0.32, 0, TWO_PI);
      ctx.stroke();
      break;
    case "people": // Gesellschaft
      ctx.arc(-s * 0.28, -s * 0.05, s * 0.32, 0, TWO_PI);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(s * 0.28, -s * 0.05, s * 0.32, 0, TWO_PI);
      ctx.stroke();
      break;
    case "user": // Persönlich
      ctx.arc(0, -s * 0.32, s * 0.32, 0, TWO_PI);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(0, s * 0.85, s * 0.6, Math.PI * 1.18, Math.PI * 1.82);
      ctx.stroke();
      break;
    case "wrench": // System und Werkzeuge
      ctx.moveTo(-s * 0.55, s * 0.55);
      ctx.lineTo(s * 0.25, -s * 0.25);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(-s * 0.6, s * 0.6, s * 0.26, 0, TWO_PI);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(s * 0.6, -s * 0.6, s * 0.26, 0, TWO_PI);
      ctx.stroke();
      break;
    case "tray": // Inbox
      ctx.moveTo(-s * 0.8, -s * 0.25);
      ctx.lineTo(-s * 0.35, s * 0.5);
      ctx.lineTo(s * 0.35, s * 0.5);
      ctx.lineTo(s * 0.8, -s * 0.25);
      ctx.moveTo(-s * 0.8, -s * 0.25);
      ctx.lineTo(s * 0.8, -s * 0.25);
      ctx.stroke();
      break;
    default:
      break;
  }
  ctx.restore();
}

/**
 * The glyph id for a node: structural for hub/skill/routine; for an area,
 * its own icon (falling back to "folder"); for a file, the icon of the
 * area it belongs to — same lookup, so a note's ring icon always matches
 * its area node's icon. Only meaningful where a file is actually big
 * enough to carry one (the orbit ring's "recent notes" icons); the tiny
 * dots elsewhere skip glyph drawing entirely (see the `n.kind !== "file"`
 * guard around the other call site).
 */
function glyphOf(n: GraphNode): string {
  if (n.kind === "area") return n.glyph ?? "folder";
  if (n.kind === "file") return glyphForArea(n.area ?? "");
  return n.kind;
}

export class GraphRenderer {
  private ctx: CanvasRenderingContext2D;
  private width = 0;
  private height = 0;
  private dpr = 1;
  model: GraphModel | null = null;
  options: RenderOptions = { spin: 0.02, labels: true, fileLabels: false, fit: 0.42 };
  view: View = { x: 0, y: 0, zoom: 1 };
  hover: GraphNode | null = null;
  selected: GraphNode | null = null;
  /** When set, only these ids draw at full strength (search results). */
  highlight: Set<string> | null = null;
  private angle = 0;
  private last = 0;
  private textColor = "#fff";
  private mutedColor = "#888";
  private lineColor = "#fff";
  private glyphColor = "#000";
  private hubGlyphColor = "#000";
  private surfaceColor = "#121216";
  private accentColor = "#ff7a1a";
  private lightScheme = false;
  /** Ring captions (rings mode). */
  captions = { skills: "SKILLS", memory: "MEMORY", routines: "ROUTINES" };
  /** In-flight `flyTo` pan/zoom tween, consumed by `frame`. */
  private flyAnim: { fromX: number; fromY: number; fromZoom: number; toX: number; toY: number; toZoom: number; start: number; duration: number } | null = null;
  /** The node `flyTo` last landed on, pulsed for a moment so it's easy to
   *  spot even once the view has stopped moving. */
  private pulse: { node: GraphNode; start: number } | null = null;
  private static readonly PULSE_DURATION = 1100; // ms

  constructor(private canvas: HTMLCanvasElement) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2d canvas context unavailable");
    this.ctx = ctx;
  }

  setColors(text: string, muted: string, line: string, glyph = "#000", hubGlyph = "#000", surface = "#121216", accent = "#ff7a1a", light = false): void {
    this.textColor = text;
    this.mutedColor = muted;
    this.lineColor = line;
    this.glyphColor = glyph;
    this.hubGlyphColor = hubGlyph;
    this.surfaceColor = surface;
    this.accentColor = accent;
    this.lightScheme = light;
  }

  resize(): void {
    const rect = this.canvas.getBoundingClientRect();
    this.dpr = window.devicePixelRatio || 1;
    this.width = Math.max(1, Math.round(rect.width));
    this.height = Math.max(1, Math.round(rect.height));
    this.canvas.width = Math.round(this.width * this.dpr);
    this.canvas.height = Math.round(this.height * this.dpr);
  }

  /** Outer radius in CSS px. */
  private radius(): number {
    return Math.min(this.width, this.height) * this.options.fit * this.view.zoom;
  }

  /** Graph units → canvas CSS px (includes spin, pan, zoom). */
  toScreen(n: { x: number; y: number }): { x: number; y: number } {
    const R = this.radius();
    const cos = Math.cos(this.angle);
    const sin = Math.sin(this.angle);
    return {
      x: this.width / 2 + this.view.x + (n.x * cos - n.y * sin) * R,
      y: this.height / 2 + this.view.y + (n.x * sin + n.y * cos) * R,
    };
  }

  /** The pan offset that puts `node` at the canvas centre at a given zoom
   *  (zoom is a parameter, not read from `this.view`, so `flyTo` can target
   *  a zoom level the view hasn't reached yet). */
  private centerFor(node: GraphNode, zoom: number): { x: number; y: number } {
    const R = Math.min(this.width, this.height) * this.options.fit * zoom;
    const cos = Math.cos(this.angle);
    const sin = Math.sin(this.angle);
    return { x: -(node.x * cos - node.y * sin) * R, y: -(node.x * sin + node.y * cos) * R };
  }

  /** Pans so `node` sits at the canvas centre (at the current zoom), no
   *  animation — used for the initial focus on open/navigate. */
  centerOn(node: GraphNode): void {
    const dest = this.centerFor(node, this.view.zoom);
    this.view.x = dest.x;
    this.view.y = dest.y;
  }

  /** The "Fly to" action: eases the pan/zoom to `node` over a beat instead
   *  of snapping, and marks it with a brief glow pulse so it's easy to spot
   *  the moment the view settles. */
  flyTo(node: GraphNode, targetZoom = Math.max(this.view.zoom, 1.6)): void {
    const dest = this.centerFor(node, targetZoom);
    const now = performance.now();
    this.flyAnim = {
      fromX: this.view.x,
      fromY: this.view.y,
      fromZoom: this.view.zoom,
      toX: dest.x,
      toY: dest.y,
      toZoom: targetZoom,
      start: now,
      duration: 550,
    };
    this.pulse = { node, start: now };
  }

  /** Nearest node within `slop` px of a CSS-px point, or null. */
  hitTest(px: number, py: number, slop = 6): GraphNode | null {
    if (!this.model) return null;
    let best: GraphNode | null = null;
    let bestD = Infinity;
    const orbit = this.options.mode === "orbit";
    const hex = this.options.mode === "hex";
    const hexPx = hex && this.model.hexUnit ? this.model.hexUnit * this.radius() : 0;
    for (const n of this.model.nodes) {
      if (orbit && n.kind === "area") continue;
      const s = orbit && n.sx !== undefined && n.sy !== undefined ? { x: n.sx, y: n.sy } : this.toScreen(n);
      const d = Math.hypot(s.x - px, s.y - py);
      const reach = hex && n.kind === "file" ? Math.max(hexPx, 3) + slop : Math.max(n.r * this.view.zoom, 3) + slop;
      if (d < reach && d < bestD) {
        best = n;
        bestD = d;
      }
    }
    return best;
  }

  frame(now: number): void {
    const dt = this.last ? Math.min(0.1, (now - this.last) / 1000) : 0;
    this.last = now;
    this.angle = (this.angle + this.options.spin * dt) % TWO_PI;
    if (this.flyAnim) {
      const a = this.flyAnim;
      const raw = Math.min(1, (now - a.start) / a.duration);
      const eased = 1 - (1 - raw) ** 3; // ease-out cubic — quick start, gentle landing
      this.view.x = a.fromX + (a.toX - a.fromX) * eased;
      this.view.y = a.fromY + (a.toY - a.fromY) * eased;
      this.view.zoom = a.fromZoom + (a.toZoom - a.fromZoom) * eased;
      if (raw >= 1) this.flyAnim = null;
    }
    this.draw(now / 1000);
  }

  private font(px: number, weight = 600): string {
    return `${weight} ${px}px ${getComputedStyle(this.canvas).fontFamily}`;
  }

  private draw(t: number): void {
    const { ctx, dpr, width, height } = this;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const model = this.model;
    if (!model) return;
    if (this.options.mode === "orbit") {
      this.drawOrbit(t);
      return;
    }
    const R = this.radius();
    const cx = width / 2 + this.view.x;
    const cy = height / 2 + this.view.y;

    // Area segments: a faint band on the outer rim; the name sits at the
    // area node (inside), counts at the band's start.
    for (const seg of model.areas) {
      const s = seg.start + this.angle;
      const e = seg.end + this.angle;
      ctx.beginPath();
      ctx.arc(cx, cy, R * 0.88, s + 0.01, e - 0.01);
      ctx.strokeStyle = seg.color;
      ctx.globalAlpha = 0.22;
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.globalAlpha = 1;
      if (this.options.labels && e - s > 0.08) {
        const a = s + 0.02;
        ctx.font = this.font(Math.max(8, Math.min(10, R * 0.035)), 500);
        ctx.fillStyle = seg.color;
        ctx.globalAlpha = 0.7;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(String(seg.count), cx + Math.cos(a) * R * 0.92, cy + Math.sin(a) * R * 0.92);
        ctx.globalAlpha = 1;
      }
    }

    // Ring captions at 12 o'clock (they don't spin — they name the rings).
    if (this.options.labels) {
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      const cap = (text: string, radius: number, color: string, size: number) => {
        ctx.font = this.font(size, 700);
        const label = text.split("").join("\u2009");
        const w = ctx.measureText(label).width;
        // Backdrop so the caption stays legible over dots and edges.
        ctx.fillStyle = this.surfaceColor;
        ctx.globalAlpha = 0.85;
        ctx.beginPath();
        ctx.roundRect(cx - w / 2 - 8, cy - radius - size - 10, w + 16, size + 8, 6);
        ctx.fill();
        ctx.fillStyle = color;
        ctx.globalAlpha = 0.9;
        ctx.fillText(label, cx, cy - radius - 6);
      };
      cap(this.captions.skills, R * 0.17 + 10, this.accentColor, Math.max(10, Math.min(15, R * 0.05)));
      cap(this.captions.memory, R * 0.84 + 4, this.mutedColor, Math.max(11, Math.min(18, R * 0.06)));
      cap(this.captions.routines, R * 0.95 + 4, this.mutedColor, Math.max(11, Math.min(18, R * 0.06)));
      ctx.globalAlpha = 1;
    }

    // Edges.
    ctx.lineWidth = 0.6;
    for (const e of model.edges) {
      const a = model.byId.get(e.from);
      const b = model.byId.get(e.to);
      if (!a || !b) continue;
      const pa = this.toScreen(a);
      const pb = this.toScreen(b);
      const hot = this.hover === a || this.hover === b || this.selected === a || this.selected === b;
      ctx.strokeStyle = hot ? a.color : a.kind === "area" ? a.color : this.lineColor;
      ctx.globalAlpha = hot ? 0.9 : a.kind === "hub" ? 0.14 : a.kind === "area" ? 0.07 : 0.2;
      ctx.beginPath();
      ctx.moveTo(pa.x, pa.y);
      ctx.lineTo(pb.x, pb.y);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;

    // Nodes.
    const hl = this.highlight;
    const hex = this.options.mode === "hex";
    const hexR = hex && model.hexUnit ? model.hexUnit * R : 0;
    for (const n of model.nodes) {
      const p = this.toScreen(n);
      const dimmed = hl !== null && !hl.has(n.id) && n !== this.selected;
      const twinkle = n.kind === "file" ? 0.75 + 0.25 * Math.sin(t * 1.7 + n.phase * TWO_PI) : 1;
      if (hex && n.kind === "file") {
        const alpha = dimmed ? 0.12 : twinkle;
        this.drawHexCell(p.x, p.y, hexR, n.color, alpha, n === this.hover || n === this.selected);
        continue;
      }
      const r = Math.max(1.2, n.r * Math.sqrt(this.view.zoom)) * (n === this.hover || n === this.selected ? 1.8 : 1);
      if (n.kind !== "file") {
        const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, r * 3.2);
        g.addColorStop(0, n.color);
        g.addColorStop(1, "transparent");
        ctx.globalAlpha = 0.35;
        ctx.fillStyle = g;
        ctx.beginPath();
        ctx.arc(p.x, p.y, r * 3.2, 0, TWO_PI);
        ctx.fill();
      }
      ctx.globalAlpha = dimmed ? 0.12 : n.kind === "routine" && n.enabled === false ? 0.35 : twinkle;
      ctx.fillStyle = n.color;
      ctx.beginPath();
      ctx.arc(p.x, p.y, r, 0, TWO_PI);
      ctx.fill();
      if (n.kind !== "file") {
        // Ring + a glyph so the kinds read at a glance (see legend).
        ctx.globalAlpha = dimmed ? 0.2 : 0.9;
        ctx.strokeStyle = n.color;
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        ctx.arc(p.x, p.y, r + 5, 0, TWO_PI);
        ctx.stroke();
        drawGlyph(ctx, glyphOf(n), p.x, p.y, r, n.kind === "hub" ? this.hubGlyphColor : this.glyphColor);
      }
    }
    ctx.globalAlpha = 1;

    // Labels.
    if (this.options.labels) {
      ctx.font = `500 ${Math.max(9, Math.min(11, R * 0.04))}px ${getComputedStyle(this.canvas).fontFamily}`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      for (const n of model.nodes) {
        const lit = hl !== null && hl.has(n.id);
        const showFile =
          n.kind === "file" && (this.options.fileLabels || lit || n === this.hover || n === this.selected);
        if (n.kind === "file" && !showFile) continue;
        if (hl !== null && !lit && n !== this.selected && n !== this.hover) continue;
        const p = this.toScreen(n);
        ctx.fillStyle = n.kind === "file" || n.kind === "hub" ? this.textColor : n.kind === "area" ? n.color : this.mutedColor;
        ctx.globalAlpha = n === this.hover || n === this.selected ? 1 : 0.8;
        const below = hex && n.kind === "file" ? hexR : n.r * this.view.zoom;
        ctx.fillText(n.label, p.x, p.y + below + 6);
      }
      ctx.globalAlpha = 1;
    }

    this.drawPulse();
  }

  /** "Fly to" lands instantly once the tween finishes; this is what makes
   *  the target actually easy to find — a couple of expanding rings plus a
   *  soft breathing glow, all fading out over `PULSE_DURATION`. */
  private drawPulse(): void {
    if (!this.pulse) return;
    const elapsedMs = performance.now() - this.pulse.start;
    if (elapsedMs > GraphRenderer.PULSE_DURATION) {
      this.pulse = null;
      return;
    }
    const { ctx } = this;
    const n = this.pulse.node;
    const p = this.toScreen(n);
    const elapsed = elapsedMs / 1000;
    const life = elapsedMs / GraphRenderer.PULSE_DURATION; // 0 → 1 over the whole pulse
    const hex = this.options.mode === "hex" && this.model?.hexUnit;
    const baseR = hex && n.kind === "file" ? this.model!.hexUnit! * this.radius() : Math.max(4, n.r * Math.sqrt(this.view.zoom));

    // Two expanding, fading rings, staggered like a double heartbeat.
    for (const phase of [0, 0.35]) {
      const local = elapsed - phase;
      if (local < 0 || local > 0.75) continue;
      const p01 = local / 0.75;
      ctx.globalAlpha = (1 - p01) * 0.7;
      ctx.strokeStyle = this.accentColor;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(p.x, p.y, baseR + p01 * baseR * 4.5, 0, TWO_PI);
      ctx.stroke();
    }
    // A gently breathing glow underneath, fading out with the pulse overall.
    const breathe = 0.3 + 0.2 * Math.sin(elapsed * Math.PI * 4);
    const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, baseR * 3.5);
    g.addColorStop(0, this.accentColor);
    g.addColorStop(1, "transparent");
    ctx.globalAlpha = Math.max(0, breathe * (1 - life));
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(p.x, p.y, baseR * 3.5, 0, TWO_PI);
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  /** Dashboard centre: dark disc with hex texture and rim, spinning 3-D
   *  particle cloud inside a wireframe geodesic, icon nodes on the rim. */
  private drawOrbit(t: number): void {
    const { ctx, width, height } = this;
    const model = this.model!;
    const R = this.radius();
    const cx = width / 2 + this.view.x;
    const cy = height / 2 + this.view.y;

    // Disc + vignette: deepens the dark themes, a whisper on light ones.
    const disc = ctx.createRadialGradient(cx, cy, R * 0.1, cx, cy, R);
    if (this.lightScheme) {
      disc.addColorStop(0, "rgba(0,0,0,0.07)");
      disc.addColorStop(0.75, "rgba(0,0,0,0.04)");
      disc.addColorStop(1, "rgba(0,0,0,0)");
    } else {
      disc.addColorStop(0, "rgba(0,0,0,0.55)");
      disc.addColorStop(0.75, "rgba(0,0,0,0.35)");
      disc.addColorStop(1, "rgba(0,0,0,0.05)");
    }
    ctx.fillStyle = disc;
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, TWO_PI);
    ctx.fill();
    // Hex texture clipped to the disc.
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, TWO_PI);
    ctx.clip();
    ctx.strokeStyle = this.lineColor;
    ctx.globalAlpha = 0.12;
    ctx.lineWidth = 0.6;
    const hexR = Math.max(14, R * 0.055);
    const hexH = Math.sqrt(3) * hexR;
    for (let row = -Math.ceil(R / hexH) - 1; row <= Math.ceil(R / hexH) + 1; row++) {
      for (let col = -Math.ceil(R / (1.5 * hexR)) - 1; col <= Math.ceil(R / (1.5 * hexR)) + 1; col++) {
        const hx = cx + col * 1.5 * hexR;
        const hy = cy + row * hexH + (col % 2 ? hexH / 2 : 0);
        if (Math.hypot(hx - cx, hy - cy) > R + hexR) continue;
        ctx.beginPath();
        for (let k = 0; k < 6; k++) {
          const a = (Math.PI / 3) * k;
          const px = hx + Math.cos(a) * hexR;
          const py = hy + Math.sin(a) * hexR;
          if (k === 0) ctx.moveTo(px, py);
          else ctx.lineTo(px, py);
        }
        ctx.closePath();
        ctx.stroke();
      }
    }
    ctx.restore();
    ctx.globalAlpha = 1;
    // Rim.
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, TWO_PI);
    ctx.strokeStyle = this.lineColor;
    ctx.globalAlpha = 0.5;
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.globalAlpha = 1;

    // 3-D rotation shared by wireframe and cloud.
    const ay = this.angle * 3;
    const ax = 0.35 + Math.sin(t * 0.15) * 0.08;
    const cosY = Math.cos(ay);
    const sinY = Math.sin(ay);
    const cosX = Math.cos(ax);
    const sinX = Math.sin(ax);
    const project = ([x, y, z]: [number, number, number]) => {
      const x1 = x * cosY - z * sinY;
      const z1 = x * sinY + z * cosY;
      const y1 = y * cosX - z1 * sinX;
      const z2 = y * sinX + z1 * cosX;
      return { x: cx + x1 * R, y: cy + y1 * R, z: z2 };
    };

    // Wireframe geodesic (icosahedron, one subdivision).
    ctx.strokeStyle = this.lineColor;
    ctx.lineWidth = 0.6;
    for (const [a, b] of GEODESIC_EDGES) {
      const pa = project(scale(GEODESIC_VERTS[a], 0.66));
      const pb = project(scale(GEODESIC_VERTS[b], 0.66));
      ctx.globalAlpha = 0.08 + 0.1 * ((pa.z + pb.z) / 2 + 1);
      ctx.beginPath();
      ctx.moveTo(pa.x, pa.y);
      ctx.lineTo(pb.x, pb.y);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;

    // Particle cloud, back to front.
    const cloud = model.nodes.filter((n) => n.kind === "file" && n.p3).map((n) => ({ n, p: project(n.p3!) }));
    cloud.sort((a, b) => a.p.z - b.p.z);
    for (const { n, p } of cloud) {
      const depth = (p.z + 1) / 2; // 0 back … 1 front
      const r = 0.9 + depth * 1.7 + (n === this.hover ? 2 : 0);
      ctx.globalAlpha = 0.25 + depth * 0.65;
      ctx.fillStyle = n.color;
      ctx.beginPath();
      ctx.arc(p.x, p.y, r, 0, TWO_PI);
      ctx.fill();
      n.sx = p.x;
      n.sy = p.y;
    }
    ctx.globalAlpha = 1;

    // Hub.
    const hub = model.byId.get("hub");
    if (hub) {
      hub.sx = cx;
      hub.sy = cy;
      const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, 18);
      g.addColorStop(0, this.accentColor);
      g.addColorStop(1, "transparent");
      ctx.globalAlpha = 0.5;
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(cx, cy, 18, 0, TWO_PI);
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.fillStyle = this.accentColor;
      ctx.beginPath();
      ctx.arc(cx, cy, 3.5, 0, TWO_PI);
      ctx.fill();
    }

    // Icon nodes on the rim.
    const nodeR = Math.max(12, Math.min(19, R * 0.062));
    ctx.font = this.font(Math.max(8, nodeR * 0.55), 600);
    for (const n of model.nodes) {
      if (!n.onOrbit) continue;
      const a = Math.atan2(n.y, n.x) + this.angle;
      const x = cx + Math.cos(a) * R;
      const y = cy + Math.sin(a) * R;
      n.sx = x;
      n.sy = y;
      const hot = n === this.hover || n === this.selected;
      if (hot) {
        const g = ctx.createRadialGradient(x, y, nodeR * 0.6, x, y, nodeR * 2.2);
        g.addColorStop(0, this.accentColor);
        g.addColorStop(1, "transparent");
        ctx.globalAlpha = 0.45;
        ctx.fillStyle = g;
        ctx.beginPath();
        ctx.arc(x, y, nodeR * 2.2, 0, TWO_PI);
        ctx.fill();
        ctx.globalAlpha = 1;
      }
      ctx.fillStyle = this.surfaceColor;
      ctx.beginPath();
      ctx.arc(x, y, nodeR, 0, TWO_PI);
      ctx.fill();
      // File nodes keep their area colour on the ring, same as everywhere
      // else in the graph — the reference "New" badge is not a category.
      ctx.strokeStyle = hot ? this.accentColor : n.color;
      ctx.lineWidth = hot ? 1.6 : 1;
      ctx.globalAlpha = hot ? 1 : 0.8;
      ctx.stroke();
      ctx.globalAlpha = 1;
      drawGlyph(ctx, glyphOf(n), x, y, nodeR * 0.62, hot ? this.accentColor : n.color);
      // Age / schedule badge under the node.
      const badge = this.badgeFor(n, t);
      if (badge) {
        ctx.fillStyle = this.mutedColor;
        ctx.globalAlpha = 0.9;
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        ctx.fillText(badge, x, y + nodeR + 3);
        ctx.globalAlpha = 1;
      }
      if (hot) {
        ctx.fillStyle = this.textColor;
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.font = this.font(Math.max(10, nodeR * 0.7), 600);
        ctx.fillText(n.label, x, y - nodeR - 6);
        ctx.font = this.font(Math.max(8, nodeR * 0.55), 600);
      }
    }
  }

  /** One honeycomb cell: a filled flat-top hexagon (matches `layoutHex`'s
   *  axial→pixel orientation) with a thin separating stroke, brightened to
   *  the accent colour when hovered/selected. */
  private drawHexCell(x: number, y: number, r: number, color: string, alpha: number, hot: boolean): void {
    const { ctx } = this;
    ctx.beginPath();
    for (let k = 0; k < 6; k++) {
      const a = (Math.PI / 3) * k;
      const px = x + Math.cos(a) * r;
      const py = y + Math.sin(a) * r;
      if (k === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.closePath();
    ctx.globalAlpha = alpha;
    ctx.fillStyle = color;
    ctx.fill();
    ctx.globalAlpha = hot ? 1 : Math.min(1, alpha + 0.3);
    ctx.strokeStyle = hot ? this.accentColor : this.surfaceColor;
    ctx.lineWidth = hot ? 1.8 : 1;
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  private badgeFor(n: GraphNode, _t: number): string | null {
    if (n.kind === "file" && n.modified) {
      const days = Math.floor((Date.now() - Date.parse(n.modified)) / 86_400_000);
      if (days < 1) return "NEW";
      return days < 30 ? `${days}D` : days < 365 ? `${Math.floor(days / 30)}M` : `${Math.floor(days / 365)}Y`;
    }
    if (n.kind === "routine") return n.enabled === false ? "OFF" : "ON";
    return null;
  }
}

function scale(v: [number, number, number], k: number): [number, number, number] {
  return [v[0] * k, v[1] * k, v[2] * k];
}

/** Icosahedron subdivided once and normalised — the geodesic wireframe. */
const [GEODESIC_VERTS, GEODESIC_EDGES] = (() => {
  const phi = (1 + Math.sqrt(5)) / 2;
  const base: [number, number, number][] = [
    [-1, phi, 0], [1, phi, 0], [-1, -phi, 0], [1, -phi, 0],
    [0, -1, phi], [0, 1, phi], [0, -1, -phi], [0, 1, -phi],
    [phi, 0, -1], [phi, 0, 1], [-phi, 0, -1], [-phi, 0, 1],
  ];
  const faces: [number, number, number][] = [
    [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
    [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
    [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
    [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
  ];
  const verts = base.map((v) => norm(v));
  const midCache = new Map<string, number>();
  const mid = (a: number, b: number) => {
    const key = a < b ? `${a}-${b}` : `${b}-${a}`;
    let i = midCache.get(key);
    if (i === undefined) {
      i = verts.length;
      verts.push(norm([(verts[a][0] + verts[b][0]) / 2, (verts[a][1] + verts[b][1]) / 2, (verts[a][2] + verts[b][2]) / 2]));
      midCache.set(key, i);
    }
    return i;
  };
  const edges = new Set<string>();
  const add = (a: number, b: number) => edges.add(a < b ? `${a}-${b}` : `${b}-${a}`);
  for (const [a, b, c] of faces) {
    const ab = mid(a, b);
    const bc = mid(b, c);
    const ca = mid(c, a);
    for (const [x, y, z] of [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]] as [number, number, number][]) {
      add(x, y);
      add(y, z);
      add(z, x);
    }
  }
  return [verts, [...edges].map((e) => e.split("-").map(Number) as [number, number])] as const;
})();

function norm(v: [number, number, number]): [number, number, number] {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}

