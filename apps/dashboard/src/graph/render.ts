/**
 * Canvas-2D renderer for the graph: area ring segments, edges, nodes with a
 * soft glow and a slow twinkle, optional labels, a slow spin, hover hit-test,
 * and a view transform (pan / zoom) the full-screen view drives. Draws with
 * the device pixel ratio; call `resize()` when the canvas box changes.
 */

import type { GraphModel, GraphNode, NodeKind } from "./model";

export interface RenderOptions {
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

/** Small vector glyph inside a non-file node: hub = hexagon, area = folder,
 *  skill = bolt, routine = clock. Drawn in the node's contrast colour. */
export function drawGlyph(
  ctx: CanvasRenderingContext2D,
  kind: NodeKind,
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
  switch (kind) {
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
    case "area":
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
    default:
      break;
  }
  ctx.restore();
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

  constructor(private canvas: HTMLCanvasElement) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2d canvas context unavailable");
    this.ctx = ctx;
  }

  setColors(text: string, muted: string, line: string, glyph = "#000", hubGlyph = "#000"): void {
    this.textColor = text;
    this.mutedColor = muted;
    this.lineColor = line;
    this.glyphColor = glyph;
    this.hubGlyphColor = hubGlyph;
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

  /** Pans so `node` sits at the canvas centre (at the current zoom). */
  centerOn(node: GraphNode): void {
    const R = this.radius();
    const cos = Math.cos(this.angle);
    const sin = Math.sin(this.angle);
    this.view.x = -(node.x * cos - node.y * sin) * R;
    this.view.y = -(node.x * sin + node.y * cos) * R;
  }

  /** Nearest node within `slop` px of a CSS-px point, or null. */
  hitTest(px: number, py: number, slop = 6): GraphNode | null {
    if (!this.model) return null;
    let best: GraphNode | null = null;
    let bestD = Infinity;
    for (const n of this.model.nodes) {
      const s = this.toScreen(n);
      const d = Math.hypot(s.x - px, s.y - py);
      const reach = Math.max(n.r * this.view.zoom, 3) + slop;
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
    this.draw(now / 1000);
  }

  private draw(t: number): void {
    const { ctx, dpr, width, height } = this;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const model = this.model;
    if (!model) return;
    const R = this.radius();
    const cx = width / 2 + this.view.x;
    const cy = height / 2 + this.view.y;

    // Area segments: a faint band plus a label on the outside.
    for (const seg of model.areas) {
      const s = seg.start + this.angle;
      const e = seg.end + this.angle;
      ctx.beginPath();
      ctx.arc(cx, cy, R * 0.9, s, e);
      ctx.strokeStyle = seg.color;
      ctx.globalAlpha = 0.28;
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.globalAlpha = 1;
      if (this.options.labels && e - s > 0.12) {
        const mid = (s + e) / 2;
        const lx = cx + Math.cos(mid) * R * 1.02;
        const ly = cy + Math.sin(mid) * R * 1.02;
        ctx.save();
        ctx.translate(lx, ly);
        const flip = Math.cos(mid) < 0;
        ctx.rotate(mid + (flip ? Math.PI : 0));
        ctx.font = `600 ${Math.max(9, Math.min(12, R * 0.045))}px ${getComputedStyle(this.canvas).fontFamily}`;
        ctx.fillStyle = seg.color;
        ctx.globalAlpha = 0.85;
        ctx.textAlign = flip ? "right" : "left";
        ctx.textBaseline = "middle";
        ctx.fillText(seg.name.toUpperCase(), 0, 0);
        ctx.restore();
        ctx.globalAlpha = 1;
      }
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
    for (const n of model.nodes) {
      const p = this.toScreen(n);
      const dimmed = hl !== null && !hl.has(n.id) && n !== this.selected;
      const twinkle = n.kind === "file" ? 0.75 + 0.25 * Math.sin(t * 1.7 + n.phase * TWO_PI) : 1;
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
        drawGlyph(ctx, n.kind, p.x, p.y, r, n.kind === "hub" ? this.hubGlyphColor : this.glyphColor);
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
        ctx.fillText(n.label, p.x, p.y + n.r * this.view.zoom + 6);
      }
      ctx.globalAlpha = 1;
    }
  }
}
