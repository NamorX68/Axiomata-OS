/**
 * Canvas-2D renderer for the graph: area ring segments, edges, nodes with a
 * soft glow and a slow twinkle, optional labels, a slow spin, hover hit-test,
 * and a view transform (pan / zoom) the full-screen view drives. Draws with
 * the device pixel ratio; call `resize()` when the canvas box changes.
 */

import type { Shine } from "../core/brainView";
import {
  agedRgb,
  buildHive,
  coastSpin,
  dragTurn,
  emberOf,
  metalLight,
  mixRgb,
  nearestCell,
  type HiveCell,
  type Rgb,
} from "./hive";
import { BUNDLE_BETA, bundleCurve, isFreshLink, linkBorn, type BundleCommand } from "./bundle";
import { APP_RING, approachAngle, bornOf, BUNDLE, EXPANDED_GROUP_RING, orbitRestAngle, RING } from "./layout";
import { glyphForArea, type GraphModel, type GraphNode } from "./model";
import { cellKey, NEIGHBOURS, pixelToAxial } from "./territory";

export type RenderMode = "bundle" | "orbit" | "map" | "hex";

export interface RenderOptions {
  /** `bundle`/`map`/`hex` = the Second Brain view (a ring of notes with bundled links, the territory map's fields, or
   *  honeycomb cells in the areas' wedges); `orbit` = the dashboard centre. */
  mode?: RenderMode;
  /** Radians per second of ring rotation. */
  spin: number;
  /** Draw skill / routine / area labels. */
  labels: boolean;
  /** Draw file titles (full view only). */
  fileLabels: boolean;
  /** Fraction of the shorter canvas side used as the outer radius. */
  fit: number;
  /** Orbit only: let the live-status pulse breathe. Off, it stays a still halo (the motion switch). */
  pulse?: boolean;
  /** Orbit only: at rest (`spin` 0) the ring jumps to its rest angle instead of turning there ("Reduce motion"). */
  snapRest?: boolean;
  /** Orbit only: how the hive's cells catch the light (G7); `metal` when unset. */
  shine?: Shine;
  /** Fade the drawing out towards the canvas's inscribed circle from this share of its radius on (0…1): the Second
   *  Brain's lens has no hard rim, it thins out into the Orbit beneath. Unset, nothing fades. */
  fadeFrom?: number;
}

/** Where the shimmer's light stands for the flat cells of a frame (`GraphRenderer.cellLight`). */
interface CellLight {
  still: boolean;
  shine: Shine;
  x: number;
  y: number;
  spread: number;
}

/** A hive cell's, map field's or honeycomb cell's tone before the shimmer (`GraphRenderer.noteTones`). */
interface HiveTone {
  col: Rgb;
  heat: number;
  /** A note that exists at the moment shown (the time travel). */
  settled: boolean;
  /** One of the hottest cells, drawn with the glow's blur. */
  blur: boolean;
}

/** One link of the bundle: its two notes, its birth (`linkBorn`) and its curve in graph units. */
interface BundleLink {
  a: GraphNode;
  b: GraphNode;
  born: number | null;
  path: BundleCommand[];
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
 * Material Symbols Rounded codepoints for every glyph id `drawGlyph` can
 * draw — see that function's own doc comment for what each id means / where
 * it comes from. Verified directly against Google's own Material Symbols
 * Rounded codepoints file (`variablefont/MaterialSymbolsRounded[...]
 * .codepoints` in google/material-design-icons), not guessed from an
 * icon's name — a wrong codepoint would silently render as tofu or an
 * unrelated icon rather than erroring.
 */
const GLYPH_CODEPOINTS: Record<string, number> = {
  hub: 0xe9f4,
  folder: 0xe2c7,
  skill: 0xea0b, // bolt
  routine: 0xefd6, // schedule
  code: 0xe86f,
  chip: 0xe322, // memory
  book: 0xea19, // menu_book
  briefcase: 0xe943, // work
  camera: 0xe412, // photo_camera
  mail: 0xe159,
  people: 0xea21, // group
  user: 0xf0d3, // person
  wrench: 0xf8cd, // build
  tray: 0xe156, // inbox
  check: 0xe668,
  calendar: 0xebcc, // calendar_month
  list: 0xe6b1, // checklist
  group: 0xe5c3, // apps — App-Ring group default
  terminal: 0xeb8e,
  kanban: 0xeb7f, // view_kanban — the same three bars as the tile icon
  ide: 0xf2e2, // developer_mode
  "code-blocks": 0xf84d, // code_blocks — the IDE's App-Ring view entry
  "edit-document": 0xf88c, // edit_document — the editor's App-Ring view entry
  graphic: 0xe40a, // palette
  calculator: 0xea5f, // calculate
  browser: 0xe80b, // public
  music: 0xe405, // music_note
  messenger: 0xe0c9, // chat
  writer: 0xe745, // edit_note
  journal: 0xe666, // auto_stories
  photos: 0xe413, // photo_library
};

/**
 * Small icon inside a non-file node, drawn in the node's contrast colour —
 * a character from the bundled Material Symbols Rounded icon font
 * (`main.ts` imports `@fontsource/material-symbols-rounded/400.css`, a
 * static weight file, deliberately not the variable-font package: canvas
 * `ctx.font` can't set `font-variation-settings`, so a variable font would
 * render at undefined default axis values). `glyph` is either a structural
 * id — "hub", "skill" (bolt), "routine" (clock), "folder" (the generic area
 * default) — or one of the per-area icons from `model.glyphForArea` (e.g.
 * "book", "code", "briefcase"), or one of the App-Ring-only icons from
 * `model.glyphForModuleType` ("check", "calendar", "list" — the others it
 * assigns are all glyphs already listed here) or the App-Ring group default
 * ("group", `core/appGroups.ts`'s `createGroup`). Unknown ids fall back to
 * "folder" (`GLYPH_CODEPOINTS` lookup miss).
 *
 * Replaces an earlier hand-drawn-vector-path version — each new icon the
 * App Ring's grouping feature needed (terminal, calculator, …) cost its own
 * manual path design, which didn't scale. Adding an icon now is a one-line
 * codepoint lookup, not new geometry. The font is loaded async like any
 * other web font; since this canvas already redraws every animation frame
 * regardless of `rebuild()` (`second-brain.svelte`'s running
 * `requestAnimationFrame` loop), an icon drawn before the font finishes
 * loading self-corrects on its own within a frame or two — no explicit
 * load-then-redraw plumbing needed.
 */
export function drawGlyph(
  ctx: CanvasRenderingContext2D,
  glyph: string,
  x: number,
  y: number,
  r: number,
  color: string,
): void {
  const codepoint = GLYPH_CODEPOINTS[glyph] ?? GLYPH_CODEPOINTS.folder;
  ctx.save();
  ctx.fillStyle = color;
  ctx.globalAlpha = 0.95;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.font = `${Math.max(8, Math.round(r * 1.9))}px "Material Symbols Rounded"`;
  ctx.fillText(String.fromCodePoint(codepoint), x, y);
  ctx.restore();
}

/** Glyph ids offered by the App-Ring group icon picker (`GlyphPicker.svelte`)
 *  — deliberately excludes "hub" (the App Ring never shows a hub node); the
 *  structural "skill"/"routine" glyphs stay in since they read fine as a
 *  group icon too. Appending a new `drawGlyph` case later just means
 *  appending it here — the picker itself never hardcodes a count, so a
 *  future expansion stage (more glyphs to choose from) is a one-line change. */
export const APP_GROUP_GLYPHS: readonly string[] = [
  "group",
  "folder",
  "code",
  "terminal",
  "ide",
  "chip",
  "graphic",
  "calculator",
  "browser",
  "book",
  "briefcase",
  "camera",
  "photos",
  "music",
  "mail",
  "messenger",
  "writer",
  "journal",
  "people",
  "user",
  "wrench",
  "tray",
  "check",
  "calendar",
  "list",
];

/**
 * The glyph id for a node: structural for hub/skill; for an area,
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
  /** Orbit only: how many card sessions are at work right now (0 = quiet, nothing is drawn). */
  activity = 0;
  /** Agents waiting for the owner (plans/agent-waiting.md): the Studio's icon then carries their number in the warn
   *  tone's hexagon instead of the count of sessions at work. */
  waiting = 0;
  /** Orbit only: the moment the time travel shows (ms), or `null` for now (G17, UX4). */
  timeCutoff: number | null = null;
  /** Sparks in flight to cells whose note just changed (W3): from the Studio's ring icon or the hub. */
  private sparks: { fileId: string; fromStudio: boolean; start: number; at: number }[] = [];
  /** Where each note's cell was last drawn, front or back, for sparks to aim at. */
  private cellPos = new Map<string, { x: number; y: number; front: boolean }>();
  private angle = 0;
  /** Whether the ring has turned on its own yet: one that starts at rest is placed at once, not turned there. */
  private hasSpun = false;
  private last = 0;
  /** Rate of the ring's ease-out into its rest angle (`approachAngle`): about a second to settle. */
  private static readonly REST_RATE = 5;
  private textColor = "#fff";
  private mutedColor = "#888";
  private lineColor = "#fff";
  private glyphColor = "#000";
  private hubGlyphColor = "#000";
  private surfaceColor = "#121216";
  private accentColor = "#ff7a1a";
  /** The glow (G2): the orange family even where the accent is another hue — the hive's ember. */
  private glowColor = "#ff7a1a";
  /** The warn tone (`--ax-warning`): "waits for the owner". */
  private warningColor = "#e6b45f";
  private lightScheme = false;
  /** The honeycomb sphere of the current model (`graph/hive.ts`); its cells are kept while the notes stay the same. */
  private hive: { model: GraphModel; signature: string; cells: HiveCell[]; nodes: (GraphNode | null)[] } | null = null;
  /** Each cell's tone, worked out at most once a second (`noteTones`), and the list of notes it was worked out for. */
  private tones: HiveTone[] = [];
  private toneList: readonly (GraphNode | null)[] | null = null;
  private toneKey = "";
  private toneAt = 0;
  /** The cells' projected positions of the current frame, reused from frame to frame. */
  private hiveXs = new Float32Array(0);
  private hiveYs = new Float32Array(0);
  private hiveZs = new Float32Array(0);
  /** Per cell of the current frame: 1 where it can be pointed at (front, a note that exists at the time shown). */
  private hivePickable = new Uint8Array(0);
  /** A cell's circumradius in CSS px in the current frame, the reach of `hitCell`. */
  private hiveCellPx = 0;
  /** The owner's own turn of the hive (radians), on top of the ambient spin: about its axis, and its tilt. */
  private hiveYaw = 0;
  private hiveTilt = 0;
  /** The yaw speed a drag let go with (radians per second), dying away in `frame` at motion Full. */
  private hiveSpin = 0;
  private turning = false;
  /** When the drag last moved the hive (`performance.now()` ms): a release after holding still does not coast. */
  private turnedAt = 0;
  /** The notes the current layout draws one by one, for the current model (`cellNotes`). */
  private cellFiles: { model: GraphModel; mode: RenderMode | undefined; files: GraphNode[] } | null = null;
  /** Where the countries' names were drawn in the last frame, for the hit test (they lie on top of the fields). */
  private mapLabels: { node: GraphNode; x: number; y: number; w: number; h: number }[] = [];
  private rgbCache = new Map<string, Rgb>();
  /** Hex's captions over the skills' ring and the notes' band. */
  captions = { skills: "SKILLS", memory: "MEMORY" };
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

  setColors(
    text: string,
    muted: string,
    line: string,
    glyph = "#000",
    hubGlyph = "#000",
    surface = "#121216",
    accent = "#ff7a1a",
    light = false,
    glow = accent,
    warning = "#e6b45f",
  ): void {
    this.glowColor = glow;
    this.warningColor = warning;
    this.rgbCache.clear();
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

  /** A file field's or honeycomb cell's px radius — 0 outside the map and
   *  the honeycomb, or before `model.hexUnit` is known. The one place
   *  `model.hexUnit` (graph units) becomes a screen size, so hit-testing,
   *  drawing and the pulse effect can't drift out of sync with each other. */
  private hexCellRadius(): number {
    const mode = this.options.mode;
    if ((mode !== "map" && mode !== "hex") || !this.model?.hexUnit) return 0;
    return this.model.hexUnit * this.radius();
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

  /** Whether `node` currently falls within the visible canvas, at least
   *  `margin` px from the edge. Used to decide whether an external "jump
   *  to this node" navigation needs to move the camera at all — with the
   *  graph's whole extent normally fitted to the canvas already, the
   *  answer is almost always yes; a bigger canvas full of far more notes
   *  than currently fit is the one case a re-centre would actually help. */
  isOnScreen(node: GraphNode, margin = 24): boolean {
    const p = this.toScreen(node);
    return p.x >= margin && p.x <= this.width - margin && p.y >= margin && p.y <= this.height - margin;
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
    if (this.options.mode === "map") return this.hitMap(px, py, slop);
    if (this.options.mode === "bundle") return this.hitBundle(px, py, slop);
    let best: GraphNode | null = null;
    let bestD = Infinity;
    const orbit = this.options.mode === "orbit";
    const hex = this.options.mode === "hex";
    const hexPx = this.hexCellRadius();
    for (const n of this.model.nodes) {
      if (orbit && n.kind === "area") continue;
      // A note of the hive is found by its cell (`hitCell`); only the recent ones also sit on the ring.
      if (orbit && n.kind === "file" && !n.onOrbit) continue;
      // In the time travel a honeycomb cell not yet born is not there to be pointed at.
      if (hex && n.kind === "file" && !this.bornBy(n)) continue;
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

  /**
   * Orbit only: the note whose hive cell lies under a CSS-px point — the nearest front cell of a note that exists at
   * the time shown, within a cell's reach — or null. The cells tile the ball, so the nearest centre is the cell the
   * pointer is on; a note on the ring is found here too, by its cell.
   */
  hitCell(px: number, py: number): GraphNode | null {
    const hive = this.hive;
    if (this.options.mode !== "orbit" || !hive) return null;
    const at = nearestCell(this.hiveXs, this.hiveYs, this.hivePickable, px, py, this.hiveCellPx);
    return at < 0 ? null : (hive.nodes[at] ?? null);
  }

  /**
   * Orbit only: turns the hive by a pointer drag of (`dx`, `dy`) CSS px over `ms` — sideways about its axis, up and
   * down by tipping it — so the surface under the pointer follows it. The ring around it stays where it is.
   */
  turnHive(dx: number, dy: number, ms: number): void {
    const { yaw, tilt } = dragTurn(this.hiveTilt, dx, dy, this.radius() * GraphRenderer.HIVE_SPHERE);
    this.hiveYaw += yaw;
    this.hiveTilt = tilt;
    this.hiveSpin = ms > 0 ? yaw / (ms / 1000) : 0;
    this.turnedAt = performance.now();
    this.turning = true;
  }

  /** The drag let go: the hive coasts on at its last speed, bounded, and slows down (motion Full only). */
  releaseHive(): void {
    this.turning = false;
    this.hiveSpin = coastSpin(this.hiveSpin, performance.now() - this.turnedAt, this.options.pulse !== false);
  }

  frame(now: number): void {
    const dt = this.last ? Math.min(0.1, (now - this.last) / 1000) : 0;
    this.last = now;
    if (!this.turning && this.hiveSpin !== 0) {
      this.hiveYaw += this.hiveSpin * dt;
      this.hiveSpin *= Math.exp(-dt * 2.5);
      if (Math.abs(this.hiveSpin) < 0.01) this.hiveSpin = 0;
    }
    // Ambient rotation pauses for as long as a node stays selected (its
    // detail panel open), not just during the "fly to" tween/pulse.
    // `centerOn`/`flyTo`'s destination is computed once, from the angle at
    // that moment — if the angle kept advancing underneath it, the camera
    // would end up next to the target instead of on it, drifting further
    // away the longer the owner actually looks at the panel (which is
    // usually well past the ~1s pulse). The node's own draw position
    // already tracks the live angle correctly every frame (so its
    // highlight ring was never actually missing, just off-centre after a
    // moment) — pausing here fixes the camera side of that mismatch rather
    // than chasing the node's math every frame.
    if (this.options.mode === "map" || this.options.mode === "bundle") {
      // A map is read north up, and the bundle's ring of names is read too: neither turns.
      this.angle = 0;
    } else if (!this.selected && !this.flyAnim && !this.pulse) {
      if (this.options.spin !== 0) {
        this.hasSpun = true;
        this.angle = (this.angle + this.options.spin * dt) % TWO_PI;
      } else if (this.options.mode === "orbit" && this.model) {
        // Paused: the ring settles with the newest note at 12 o'clock (re-aimed every frame, so a new note moves it).
        const rest = orbitRestAngle(this.model);
        this.angle =
          this.options.snapRest || !this.hasSpun ? rest : approachAngle(this.angle, rest, dt, GraphRenderer.REST_RATE);
      }
    }
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
    if (this.options.fadeFrom !== undefined) this.fadeEdge(this.options.fadeFrom);
  }

  /**
   * Thins the finished frame out towards the canvas's inscribed circle, from `from` of its radius on: what was drawn
   * keeps only as much as a radial gradient lets through. Done on the canvas, not with a CSS mask, which WebKit and
   * Chromium leave the canvas blank with inside the Orbit's container.
   */
  private fadeEdge(from: number): void {
    const { ctx, width, height } = this;
    const r = Math.min(width, height) / 2;
    const g = ctx.createRadialGradient(width / 2, height / 2, r * from, width / 2, height / 2, r);
    g.addColorStop(0, "#000");
    g.addColorStop(1, "transparent");
    ctx.save();
    ctx.globalCompositeOperation = "destination-in";
    ctx.globalAlpha = 1;
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, width, height);
    ctx.restore();
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
    if (this.options.mode === "map") {
      this.drawMap(t);
      return;
    }
    if (this.options.mode === "bundle") {
      this.drawBundle(t);
      return;
    }
    this.drawHex(t);
  }

  /**
   * The honeycomb (Hex): hub, skills and area markers in the middle with faint spokes, every note a cell in its area's
   * wedge in its tone (`noteTones`) — in the time travel only the notes born by then.
   */
  private drawHex(t: number): void {
    const { ctx, width, height } = this;
    const model = this.model!;
    const R = this.radius();
    const cx = width / 2 + this.view.x;
    const cy = height / 2 + this.view.y;

    // Captions at 12 o'clock naming the skills' ring and the notes' band (they don't spin).
    if (this.options.labels) {
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      const cap = (text: string, radius: number, color: string, size: number) => {
        ctx.font = this.font(size, 700);
        const label = text.split("").join("\u2009");
        const w = ctx.measureText(label).width;
        // Backdrop so the caption stays legible over cells and edges.
        ctx.fillStyle = this.surfaceColor;
        ctx.globalAlpha = 0.85;
        ctx.beginPath();
        ctx.roundRect(cx - w / 2 - 8, cy - radius - size - 10, w + 16, size + 8, 6);
        ctx.fill();
        ctx.fillStyle = color;
        ctx.globalAlpha = 0.9;
        ctx.fillText(label, cx, cy - radius - 6);
      };
      cap(this.captions.skills, R * RING.skills + 10, this.accentColor, Math.max(10, Math.min(15, R * 0.05)));
      cap(this.captions.memory, R * RING.filesOuter + 4, this.mutedColor, Math.max(11, Math.min(18, R * 0.06)));
      ctx.globalAlpha = 1;
    }

    const hexR = this.hexCellRadius();
    const cellNotes = this.cellNotes(model);
    const cellTones = this.noteTones(cellNotes);
    const unborn = (n: GraphNode) => n.kind === "file" && !this.bornBy(n);

    // Edges: the hub's spokes, each area's to its notes, and the links between notes.
    ctx.lineWidth = 0.6;
    for (const e of model.edges) {
      const a = model.byId.get(e.from);
      const b = model.byId.get(e.to);
      if (!a || !b || unborn(a) || unborn(b)) continue;
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

    // The cells, then the hub, skills and areas over them.
    const hl = this.highlight;
    const light = this.cellLight(t);
    cellNotes.forEach((n, i) => {
      const tone = cellTones[i];
      if (!tone?.settled) return;
      const p = this.toScreen(n);
      const dimmed = hl !== null && !hl.has(n.id) && n !== this.selected;
      const fill = this.shimmerFill(tone.col, p.x, p.y, n.phase, t, light);
      const hot = n === this.selected ? "selected" : n === this.hover ? "hover" : null;
      this.drawHexCell(p.x, p.y, hexR, fill, dimmed ? 0.12 : 1, hot, tone);
    });
    for (const n of model.nodes) {
      if (n.kind === "file") continue;
      const p = this.toScreen(n);
      const dimmed = hl !== null && !hl.has(n.id) && n !== this.selected;
      const r = Math.max(1.2, n.r * Math.sqrt(this.view.zoom)) * (n === this.hover || n === this.selected ? 1.8 : 1);
      const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, r * 3.2);
      g.addColorStop(0, n.color);
      g.addColorStop(1, "transparent");
      ctx.globalAlpha = 0.35;
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(p.x, p.y, r * 3.2, 0, TWO_PI);
      ctx.fill();
      ctx.globalAlpha = dimmed ? 0.12 : 1;
      ctx.fillStyle = n.color;
      ctx.beginPath();
      ctx.arc(p.x, p.y, r, 0, TWO_PI);
      ctx.fill();
      // Ring + a glyph so the kinds read at a glance (see legend).
      ctx.globalAlpha = dimmed ? 0.2 : 0.9;
      ctx.strokeStyle = n.color;
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.arc(p.x, p.y, r + 5, 0, TWO_PI);
      ctx.stroke();
      drawGlyph(ctx, glyphOf(n), p.x, p.y, r, n.kind === "hub" ? this.hubGlyphColor : this.glyphColor);
    }
    ctx.globalAlpha = 1;

    // Labels.
    if (this.options.labels) {
      ctx.font = `500 ${this.labelPx(R)}px ${getComputedStyle(this.canvas).fontFamily}`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      for (const n of model.nodes) {
        const lit = hl !== null && hl.has(n.id);
        const showFile =
          n.kind === "file" && (this.options.fileLabels || lit || n === this.hover || n === this.selected);
        if (n.kind === "file" && !showFile) continue;
        if (hl !== null && !lit && n !== this.selected && n !== this.hover) continue;
        if (unborn(n)) continue;
        const p = this.toScreen(n);
        ctx.fillStyle = n.kind === "file" || n.kind === "hub" ? this.textColor : n.kind === "area" ? n.color : this.mutedColor;
        ctx.globalAlpha = n === this.hover || n === this.selected ? 1 : 0.8;
        const below = n.kind === "file" ? hexR : n.r * this.view.zoom;
        ctx.fillText(n.label, p.x, p.y + below + 6);
      }
      ctx.globalAlpha = 1;
    }

    this.drawPulse();
  }

  /** A caption's px size for a ring of radius `R` px: it grows when zoomed in, gently (square root), up to the map's
   *  largest field name. */
  private labelPx(R: number): number {
    const base = Math.max(9, Math.min(11, R * 0.04));
    return Math.min(GraphRenderer.FIELD_NAME_MAX_PX, base * Math.max(1, Math.sqrt(this.view.zoom)));
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
    const hexPx = this.hexCellRadius();
    const baseR = hexPx > 0 && n.kind === "file" ? hexPx : Math.max(4, n.r * Math.sqrt(this.view.zoom));

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

    // Disc + vignette: deepens the dark themes; light ones (paper is the
    // only one) got a mere whisper here originally, which read as barely
    // there against a warm cream background — strengthened so the disc
    // still reads as a distinct field, not just scattered dots.
    const disc = ctx.createRadialGradient(cx, cy, R * 0.1, cx, cy, R);
    if (this.lightScheme) {
      disc.addColorStop(0, "rgba(0,0,0,0.14)");
      disc.addColorStop(0.75, "rgba(0,0,0,0.08)");
      disc.addColorStop(1, "rgba(0,0,0,0.02)");
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
    // Same low-contrast-on-cream reason as the vignette above: the hex
    // texture needs more bite in the light scheme to stay perceptible.
    ctx.globalAlpha = this.lightScheme ? 0.22 : 0.12;
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
    // Rim. In the light scheme, `lineColor` (the theme's border colour) sits
    // too close in luminance to the cream background to read as an edge at
    // all — traced in the accent colour instead, which also gives the disc
    // some of the theme's own identity rather than a flat grey ring.
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, TWO_PI);
    ctx.strokeStyle = this.lightScheme ? this.accentColor : this.lineColor;
    ctx.globalAlpha = this.lightScheme ? 0.55 : 0.5;
    ctx.lineWidth = this.lightScheme ? 1.4 : 1;
    ctx.stroke();
    ctx.globalAlpha = 1;

    // The hive's 3-D rotation: the ambient spin plus the owner's own turn (`turnHive`).
    const ay = this.angle * 3 + this.hiveYaw;
    const ax = 0.35 + this.hiveTilt + Math.sin(t * 0.15) * 0.08;
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

    this.drawHive(cx, cy, R, t, project);

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

    this.drawHeartbeat(cx, cy, R, t);

    // Icon nodes on the rim.
    const onOrbitCount = model.nodes.reduce((n, node) => n + (node.onOrbit ? 1 : 0), 0);
    const nodeR = rimNodeRadiusPx(R, onOrbitCount);
    ctx.font = this.font(Math.max(8, nodeR * 0.55), 600);
    for (const n of model.nodes) {
      if (!n.onOrbit) continue;
      const a = Math.atan2(n.y, n.x) + this.angle;
      const x = cx + Math.cos(a) * R;
      const y = cy + Math.sin(a) * R;
      n.sx = x;
      n.sy = y;
      const hot = n === this.hover || n === this.selected;
      this.drawNodeDisc(x, y, nodeR, n.color, hot);
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
      if (hot) this.drawHotLabel(x, y, nodeR, n.label, 0.7, 0.55);
    }

    this.drawAppRing(cx, cy, R, t);
    this.drawExpandedGroupRing(cx, cy, R);
    this.drawSparks(cx, cy, t);
  }

  /** How long a spark takes from its source to the cell, in seconds. */
  private static readonly SPARK_FLIGHT = 1.1;

  /**
   * A note changed (`vault:changed`, G9): a spark flies to its cell — from the Studio's ring icon while card sessions
   * work, else from the hub — and the cell takes the glow when it lands. At Calm and Off nothing flies: the note is
   * marked touched at once (G3).
   */
  spark(fileId: string, at: number): void {
    const node = this.model?.byId.get(fileId);
    if (!node) return;
    if (this.options.pulse === false) {
      this.touch(fileId, at);
      return;
    }
    this.sparks.push({ fileId, fromStudio: this.activity > 0, start: performance.now() / 1000, at });
  }

  /** Marks a note as changed at `at` without a spark: its cell takes the glow at once (the rest of a large burst). */
  touch(fileId: string, at: number): void {
    const node = this.model?.byId.get(fileId);
    if (!node) return;
    node.modified = new Date(at).toISOString();
    // Its cell glows from the next frame on, not after the tones' next second.
    this.toneAt = 0;
  }

  private drawSparks(cx: number, cy: number, t: number): void {
    if (this.sparks.length === 0) return;
    const { ctx } = this;
    const studio = this.model?.nodes.find((n) => n.appType === "view:ide");
    const now = performance.now() / 1000;
    const flying: typeof this.sparks = [];
    for (const s of this.sparks) {
      const u = (now - s.start) / GraphRenderer.SPARK_FLIGHT;
      const node = this.model?.byId.get(s.fileId);
      const target = this.cellPos.get(s.fileId);
      if (!node || !target) continue;
      // A cell turned away cannot be seen to be hit: it glows without a spark.
      if (u >= 1 || !target.front) {
        this.touch(s.fileId, s.at);
        continue;
      }
      flying.push(s);
      const fromStudio = s.fromStudio && studio?.sx !== undefined && studio.sy !== undefined;
      const from = fromStudio ? { x: studio.sx ?? cx, y: studio.sy ?? cy } : { x: cx, y: cy };
      // A curve bowed towards the centre and up, so a spark from the ring arcs over the ball rather than cutting it.
      const mx = (from.x + target.x) / 2 + (cx - (from.x + target.x) / 2) * 0.5;
      const my = (from.y + target.y) / 2 - 40;
      const point = (k: number) => ({
        x: (1 - k) ** 2 * from.x + 2 * (1 - k) * k * mx + k * k * target.x,
        y: (1 - k) ** 2 * from.y + 2 * (1 - k) * k * my + k * k * target.y,
      });
      const ease = 1 - (1 - u) ** 2;
      // The trail: short segments fading towards its tail.
      ctx.strokeStyle = this.glowColor;
      ctx.lineWidth = 1.5;
      const tail = Math.max(0, ease - 0.25);
      let prev = point(tail);
      for (let k = tail + 0.02; k <= ease; k += 0.02) {
        const p = point(k);
        ctx.globalAlpha = 0.15 + 0.85 * ((k - tail) / 0.25);
        ctx.beginPath();
        ctx.moveTo(prev.x, prev.y);
        ctx.lineTo(p.x, p.y);
        ctx.stroke();
        prev = p;
      }
      const head = point(ease);
      ctx.globalAlpha = 1;
      ctx.shadowBlur = this.lightScheme ? 4 : 12;
      ctx.shadowColor = this.glowColor;
      ctx.fillStyle = this.glowColor;
      ctx.beginPath();
      ctx.arc(head.x, head.y, 2.5 + Math.sin(t * 20) * 0.4, 0, TWO_PI);
      ctx.fill();
      ctx.shadowBlur = 0;
    }
    ctx.globalAlpha = 1;
    this.sparks = flying;
  }

  /** The sphere's radius as a share of the orbit ring's. */
  static readonly HIVE_SPHERE = 0.62;

  /**
   * The honeycomb sphere (`graph/hive.ts`, UX4 W1/W2, G7) in place of the particle cloud: every note a cell in its
   * area's colour — the one its icon has on the ring —, with an ember where it was touched today and darker the longer
   * nothing moved; the spare
   * cells dark and unsettled. Under "metal" each front cell is lit by its tilt, with a highlight; "twinkle" lets each
   * breathe; "calm" holds still — and so does every light while the motion is not Full (`options.pulse`).
   */
  private drawHive(
    cx: number,
    cy: number,
    R: number,
    t: number,
    project: (p: [number, number, number]) => { x: number; y: number; z: number },
  ): void {
    const { ctx } = this;
    const model = this.model!;
    if (this.hive?.model !== model) this.rebuildHive(model);
    const hive = this.hive!;
    const tones = this.noteTones(hive.nodes);
    const sphere = GraphRenderer.HIVE_SPHERE;
    const still = this.options.pulse === false;
    const shine = this.options.shine ?? "metal";
    const size = R * sphere * Math.sqrt((4 * Math.PI) / hive.cells.length) * 0.55;
    const n = hive.cells.length;
    if (this.hiveXs.length !== n) {
      this.hiveXs = new Float32Array(n);
      this.hiveYs = new Float32Array(n);
      this.hiveZs = new Float32Array(n);
      this.hivePickable = new Uint8Array(n);
    }
    this.hiveCellPx = size;
    const xs = this.hiveXs;
    const ys = this.hiveYs;
    const zs = this.hiveZs;
    for (let i = 0; i < n; i++) {
      const p = project(scale(hive.cells[i]!.p, sphere));
      xs[i] = p.x;
      ys[i] = p.y;
      zs[i] = p.z / sphere;
    }

    // The far side first, then the near one: on a ball that is all the ordering the cells need.
    for (const nearSide of [false, true]) {
      for (let i = 0; i < n; i++) {
        const nz = zs[i]!;
        const front = nz > 0;
        if (front !== nearSide) continue;
        const cell = hive.cells[i]!;
        const node = hive.nodes[i] ?? null;
        const tone = tones[i]!;
        const x = xs[i]!;
        const y = ys[i]!;
        // The cell's normal in screen space: its own direction from the sphere's centre.
        const nx = (x - cx) / (R * sphere);
        const ny = (y - cy) / (R * sphere);
        let [r, g, b] = tone.col;
        if (front && shine !== "calm") {
          const wave = Math.sin(t * 1.7 + cell.phase * TWO_PI);
          const twinkle = still ? 1 : 0.82 + 0.18 * wave;
          if (shine === "twinkle") {
            r *= twinkle;
            g *= twinkle;
            b *= twinkle;
          } else {
            const { shade, spec } = metalLight(nx, ny, nz, still ? 1 : 0.6 + 0.4 * wave);
            const k = shade * twinkle;
            r = Math.min(255, r * k + (255 - r) * spec * 0.45);
            g = Math.min(255, g * k + (255 - g) * spec * 0.45);
            b = Math.min(255, b * k + (255 - b) * spec * 0.45);
          }
        }
        ctx.save();
        ctx.translate(x, y);
        // Foreshortened towards the rim, as a flat plate on a ball would be.
        ctx.rotate(Math.atan2(ny, nx));
        ctx.scale(Math.max(Math.abs(nz), 0.12), 1);
        this.hexagonPath(size * (front ? 0.92 : 0.7));
        ctx.globalAlpha = front ? (tone.settled ? 1 : 0.55) : 0.13;
        ctx.fillStyle = `rgb(${Math.round(r)},${Math.round(g)},${Math.round(b)})`;
        ctx.fill();
        // A fresh note's glow: an ember in the middle of its cell, which itself keeps its area's colour.
        if (front) this.drawEmber(ctx, 0, 0, size, 1, tone);
        ctx.restore();
        // Only a front cell of a note that exists can be pointed at (`hitCell`).
        this.hivePickable[i] = node && front && tone.settled ? 1 : 0;
        if (node) this.cellPos.set(node.id, { x, y, front: front && tone.settled });
      }
    }
    ctx.globalAlpha = 1;

    // The hovered note's cell — also when it was hovered on the ring, which shows where on the ball it lives.
    const hovered = this.hover?.kind === "file" ? this.cellPos.get(this.hover.id) : undefined;
    if (hovered?.front) {
      ctx.save();
      ctx.translate(hovered.x, hovered.y);
      this.hexagonPath(size * 1.3);
      ctx.strokeStyle = this.textColor;
      ctx.lineWidth = 1.5;
      ctx.stroke();
      ctx.restore();
    }
  }

  /** How long a cell's tone (colour, heat) is kept before it is worked out again: heat fades by the hour. */
  private static readonly TONE_TTL_MS = 1000;
  /** The most cells drawn with the glow's blur at once — a blur per cell is the costly part after a large sync. */
  private static readonly MAX_BLURRED = 150;

  /**
   * The hive for a new model. The 30-second refresh makes a new model with the same notes most of the time: then the
   * sphere is kept and only its cells are tied to the new nodes, instead of growing the ball again.
   */
  private rebuildHive(model: GraphModel): void {
    const files = model.nodes.filter((n) => n.kind === "file");
    const signature = files.map((n) => `${n.id}\u0001${n.area ?? ""}`).join("\u0002");
    const kept = this.hive?.signature === signature ? this.hive.cells : null;
    const cells = kept ?? buildHive(files.map((n) => ({ id: n.id, area: n.area, born: bornOf(n) })));
    const nodes = cells.map((c) => (c.fileId ? (model.byId.get(c.fileId) ?? null) : null));
    this.hive = { model, signature, cells, nodes };
    this.cellPos.clear();
    this.toneAt = 0;
  }

  /**
   * Each note's colour before the shimmer, its heat, whether it is settled and whether it gets the glow's blur — for
   * the hive's cells (`null` = an unsettled one) and the map's fields alike — worked out at most once a second (and at
   * once after a change of colours, of the list, the time travel or a landed spark), not in every frame. A note keeps
   * its area's colour, the one its icon has on the ring: age only darkens it (`agedRgb`), and its heat is drawn as an
   * ember on top. `cooling` is how far an old note darkens (1 = all the way): a map keeps more of its countries' light.
   */
  private noteTones(nodes: readonly (GraphNode | null)[], cooling = 1): HiveTone[] {
    const now = Date.now();
    const key = `${this.surfaceColor}|${this.mutedColor}|${this.timeCutoff}|${cooling}`;
    const fresh = now - this.toneAt < GraphRenderer.TONE_TTL_MS;
    if (this.toneList === nodes && key === this.toneKey && fresh) return this.tones;
    const surface = this.rgb(this.surfaceColor);
    const unsettled = mixRgb(surface, this.rgb(this.mutedColor), 0.12);
    const cutoff = this.timeCutoff;
    const tones: HiveTone[] = nodes.map((node) => {
      // In the time travel a note not yet born is an unsettled cell, and one born just then glows: the vault grows.
      const born = node?.created ? Date.parse(node.created) : null;
      const settled = node !== null && (cutoff === null || born === null || born <= cutoff);
      if (!node || !settled) return { col: unsettled, heat: 0, settled: false, blur: false };
      const touched = cutoff === null ? (node.modified ? Date.parse(node.modified) : null) : born;
      const glow = emberOf(touched, cutoff ?? now);
      return { col: agedRgb(this.rgb(node.color), glow.ash * cooling), heat: glow.heat, settled: true, blur: false };
    });
    tones
      .filter((tone) => tone.heat > 0.2)
      .sort((a, b) => b.heat - a.heat)
      .slice(0, GraphRenderer.MAX_BLURRED)
      .forEach((tone) => (tone.blur = true));
    this.tones = tones;
    this.toneList = nodes;
    this.toneKey = key;
    this.toneAt = now;
    return tones;
  }

  /** A pointy-top hexagon of circumradius `r` around the current origin, as the current path. */
  private hexagonPath(r: number): void {
    const { ctx } = this;
    ctx.beginPath();
    for (let k = 0; k < 6; k++) {
      const a = (Math.PI / 3) * k - Math.PI / 2;
      ctx.lineTo(Math.cos(a) * r, Math.sin(a) * r);
    }
    ctx.closePath();
  }

  /** A CSS colour as RGB, read through the canvas (which normalises any notation), cached per colour string. */
  private rgb(color: string): Rgb {
    const cached = this.rgbCache.get(color);
    if (cached) return cached;
    const { ctx } = this;
    ctx.save();
    ctx.fillStyle = "#000";
    ctx.fillStyle = color;
    const norm = String(ctx.fillStyle);
    ctx.restore();
    let rgb: Rgb = [128, 128, 128];
    const hex = /^#([0-9a-f]{6})$/i.exec(norm);
    const fn = /^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/i.exec(norm);
    if (hex?.[1]) {
      const v = parseInt(hex[1], 16);
      rgb = [(v >> 16) & 255, (v >> 8) & 255, v & 255];
    } else if (fn) {
      rgb = [Number(fn[1]), Number(fn[2]), Number(fn[3])];
    }
    this.rgbCache.set(color, rgb);
    return rgb;
  }

  /** One ring node's disc, shared by the inner orbit ring's rim loop and the
   *  App Ring below: an accent hover/selection halo (only when `hot`), a
   *  surface-filled circle, and a border (accent when `hot`, `color`
   *  otherwise) — everything each ring draws before its own glyph/icon goes
   *  on top. Extracted because the two rings' node-circle code was an exact
   *  duplicate (App Ring's icons/monogram and the orbit ring's glyph/badge
   *  are genuinely ring-specific and stay in their own methods). */
  private drawNodeDisc(x: number, y: number, nodeR: number, color: string, hot: boolean): void {
    const { ctx } = this;
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
    ctx.strokeStyle = hot ? this.accentColor : color;
    ctx.lineWidth = hot ? 1.6 : 1;
    ctx.globalAlpha = hot ? 1 : 0.8;
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  /** Draws `label` above a hot (hovered/selected) node, then restores the
   *  ring's own resting-size font — shared by both rings, which only differ
   *  in how large their resting vs. hot text reads relative to their own
   *  `nodeR` (`hotScale`/`restingScale`). */
  private drawHotLabel(
    x: number,
    y: number,
    nodeR: number,
    label: string,
    hotScale: number,
    restingScale: number,
  ): void {
    const { ctx } = this;
    ctx.fillStyle = this.textColor;
    ctx.textAlign = "center";
    ctx.textBaseline = "bottom";
    ctx.font = this.font(Math.max(10, nodeR * hotScale), 600);
    ctx.fillText(label, x, y - nodeR - 6);
    ctx.font = this.font(Math.max(8, nodeR * restingScale), 600);
  }

  /** The App Ring: a second, static ring just outside the one above —
   *  builtin modules left of the "+", externally added Mac apps right of
   *  it. Deliberately outside the "Icon nodes on the rim" loop (these nodes
   *  never set `onOrbit`, so that loop already skips them) and drawn with
   *  no `+ this.angle` term, so the ring doesn't rotate with the ambient
   *  spin the way the loop above does (see `layout.ts`'s `layoutAppRing`
   *  doc comment for why that's required, not just the lack of a time
   *  dependency in the layout itself). */
  private drawAppRing(cx: number, cy: number, R: number, t: number): void {
    const { ctx } = this;
    const model = this.model!;
    const ringR = R * APP_RING;

    // Subtle separation circle, distinguishing this ring from the one it
    // surrounds without competing with it for attention.
    ctx.beginPath();
    ctx.arc(cx, cy, ringR, 0, TWO_PI);
    ctx.strokeStyle = this.lineColor;
    ctx.globalAlpha = 0.25;
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.globalAlpha = 1;

    const nodeR = appNodeRadiusPx(R);
    ctx.font = this.font(Math.max(8, nodeR * 0.5), 600);
    for (const n of model.nodes) {
      // Expanded-ring members are drawn by `drawExpandedGroupRing` instead,
      // further out — a collapsed group's members never even reach this
      // loop (`model.ts`'s `buildAppNodes` only emits them while their
      // group is the expanded one), but a currently-expanded group's own
      // members would otherwise also match `n.kind === "app"` here.
      if (n.kind !== "app" || n.onExpandedRing) continue;
      const a = Math.atan2(n.y, n.x); // static — no `+ this.angle`
      const x = cx + Math.cos(a) * ringR;
      const y = cy + Math.sin(a) * ringR;
      n.sx = x;
      n.sy = y;
      const hot = n === this.hover || n === this.selected;
      // A group's own circle reads as "somewhat bigger" than a solo app
      // icon (the owner's own framing when asking for grouping) — the
      // disc/glyph/hot-label below all key off this per-node `r` instead of
      // the shared `nodeR`, so they scale together consistently.
      const r = n.isGroup ? nodeR * 1.25 : nodeR;
      const working = n.appType === "view:ide" && this.activity > 0;
      if (working) this.drawActivityHalo(x, y, r, t);
      this.drawNodeDisc(x, y, r, n.color, hot);

      if (n.userApp && !n.glyph) {
        // No icon extraction for Mac apps by default — a monogram stands
        // in until the owner picks one via "Symbol ändern" (`n.glyph` set,
        // `core/apps.ts`'s `setUserAppGlyph`). A user-side *group* node
        // also has `userApp: true` (so the App Ring's left/right split
        // keeps working for it too) but always has a glyph (`"group"`
        // default), so it never falls into this branch either.
        //
        // A real-icon-as-silhouette feature was attempted here (see the
        // App-Ring-Gruppierung follow-up plan) — the underlying pipeline
        // (a new `get_app_icon` Tauri command, base64 PNG, canvas
        // `drawImage`) was proven working live (Safari's icon rendered
        // correctly), but debugging it was very costly (an early test app
        // happened to have a nearly all-white/transparent icon, which
        // looked exactly like "drawImage silently paints nothing" for a
        // long time) and the owner chose to revert rather than build it
        // out further for now. Kept here as a known "not pursued, not
        // proven broken" item, alongside the tile mouse-wheel-scroll bug.
        ctx.fillStyle = hot ? this.accentColor : n.color;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(monogram(n.label), x, y + 1);
      } else {
        drawGlyph(ctx, n.glyph ?? "folder", x, y, r * 0.62, hot ? this.accentColor : n.color);
      }
      if (n.appType === "view:ide" && this.waiting > 0) this.drawWaitingBadge(x, y, r);
      else if (working) this.drawActivityBadge(x, y, r);

      if (hot) this.drawHotLabel(x, y, r, n.label, 0.75, 0.5);
    }
  }

  /** Seconds one breath of the live-status pulse takes — slow enough to stay in the corner of the eye. */
  private static readonly PULSE_PERIOD = 3.6;

  /** 0…1 position within the current breath; a still 0.5 while the motion is off. */
  private pulsePhase(t: number): number {
    return this.options.pulse === false ? 0.5 : (t % GraphRenderer.PULSE_PERIOD) / GraphRenderer.PULSE_PERIOD;
  }

  /** Quiet heartbeat: while sessions work, a faint ring leaves the cloud's centre and fades before it reaches the rim.
   *  Not drawn while the motion is off — a ring frozen mid-way would look like a stray circle. */
  private drawHeartbeat(cx: number, cy: number, R: number, t: number): void {
    if (this.activity <= 0 || this.options.pulse === false) return;
    const { ctx } = this;
    const phase = this.pulsePhase(t);
    ctx.beginPath();
    ctx.arc(cx, cy, 6 + (R * 0.62 - 6) * phase, 0, TWO_PI);
    ctx.strokeStyle = this.glowColor;
    ctx.globalAlpha = 0.32 * (1 - phase) ** 2;
    ctx.lineWidth = 1.5;
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  /** A soft accent glow behind the Studio's icon: breathes with the heartbeat, or stays lit when the motion is off. */
  private drawActivityHalo(x: number, y: number, r: number, t: number): void {
    const { ctx } = this;
    const breath = this.options.pulse === false ? 0.6 : 0.5 + 0.5 * Math.sin(this.pulsePhase(t) * TWO_PI);
    const g = ctx.createRadialGradient(x, y, r * 0.5, x, y, r * 2.4);
    g.addColorStop(0, this.glowColor);
    g.addColorStop(1, "transparent");
    ctx.globalAlpha = 0.25 + 0.3 * breath;
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(x, y, r * 2.4, 0, TWO_PI);
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  /** The number of agents waiting for the owner, in a hexagon of the warn tone at the icon's upper right — the same
   *  sign as the Dock's and the Kanban's badge; it stands in for the count of sessions at work while anyone waits. */
  private drawWaitingBadge(x: number, y: number, r: number): void {
    const { ctx } = this;
    const br = Math.max(7, r * 0.48);
    const bx = x + r * 0.78;
    const by = y - r * 0.78;
    ctx.fillStyle = this.warningColor;
    ctx.beginPath();
    // Pointy-top, like `--ax-hex-clip`.
    for (let i = 0; i < 6; i++) {
      const a = -Math.PI / 2 + (i * Math.PI) / 3;
      const px = bx + br * Math.cos(a);
      const py = by + br * Math.sin(a);
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = this.glyphColor;
    ctx.font = this.font(Math.max(8, br * 1.05), 700);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(this.waiting > 9 ? "9+" : String(this.waiting), bx, by + 0.5);
  }

  /** The number of sessions at work, in a small accent dot at the icon's upper right. */
  private drawActivityBadge(x: number, y: number, r: number): void {
    const { ctx } = this;
    const br = Math.max(6, r * 0.4);
    const bx = x + r * 0.78;
    const by = y - r * 0.78;
    ctx.fillStyle = this.glowColor;
    ctx.beginPath();
    ctx.arc(bx, by, br, 0, TWO_PI);
    ctx.fill();
    ctx.fillStyle = this.glyphColor;
    ctx.font = this.font(Math.max(8, br * 1.15), 700);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(this.activity > 9 ? "9+" : String(this.activity), bx, by + 0.5);
  }

  /** A group's expanded secondary ring: its member nodes (`onExpandedRing`),
   *  drawn at `EXPANDED_GROUP_RING` — further out than the App Ring itself
   *  — with the exact same per-node drawing `drawAppRing` uses for a solo
   *  app, so a member looks identical whether it's shown here or (once
   *  ungrouped) back on the App Ring proper. Called right after
   *  `drawAppRing` so the App Ring's own separation circle/icons are
   *  already down first. */
  private drawExpandedGroupRing(cx: number, cy: number, R: number): void {
    const { ctx } = this;
    const model = this.model!;
    const ringR = R * EXPANDED_GROUP_RING;
    const nodeR = appNodeRadiusPx(R);
    ctx.font = this.font(Math.max(8, nodeR * 0.5), 600);
    for (const n of model.nodes) {
      if (!n.onExpandedRing) continue;
      const a = Math.atan2(n.y, n.x);
      const x = cx + Math.cos(a) * ringR;
      const y = cy + Math.sin(a) * ringR;
      n.sx = x;
      n.sy = y;
      const hot = n === this.hover || n === this.selected;
      this.drawNodeDisc(x, y, nodeR, n.color, hot);

      if (n.userApp && !n.glyph) {
        ctx.fillStyle = hot ? this.accentColor : n.color;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(monogram(n.label), x, y + 1);
      } else {
        drawGlyph(ctx, n.glyph ?? "folder", x, y, nodeR * 0.62, hot ? this.accentColor : n.color);
      }

      if (hot) this.drawHotLabel(x, y, nodeR, n.label, 0.75, 0.5);
    }
  }

  /** From this zoom on the links show as roads between the fields; they are fully there half a step later. */
  private static readonly ROADS_FROM = 1.3;
  /** From this zoom on every field carries its note's name. */
  private static readonly NAMES_FROM = 2.4;
  /** Up to this zoom the countries carry their names; they fade out on the way there. */
  private static readonly COUNTRIES_UNTIL = 2.8;
  /** How far an old note darkens on the map: half way, so every country keeps its light. */
  private static readonly MAP_COOLING = 0.5;
  /** The largest a field's name grows when zoomed in close (px): grows with the field, so it stays readable up
   *  close (owner, 2026-10-29: 12 px read small there). */
  private static readonly FIELD_NAME_MAX_PX = 17;

  /** The least heat (0…1) a field or cell shows as an ember core. */
  private static readonly EMBER_FROM = 0.05;
  /** An ember's radius as a share of its cell's: a hint of heat, not a second mark over the area's colour (owner,
   *  2026-10-29: at 0.22 it was far too big). */
  private static readonly EMBER_SIZE = 0.09;

  /** A fresh note's ember at `(x, y)` in a cell of radius `cell`, as bright as the note is fresh; nothing for a cool
   *  one. Returns whether it drew (it leaves `globalAlpha` changed). */
  private drawEmber(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    cell: number,
    alpha: number,
    tone: { heat: number; blur: boolean },
  ): boolean {
    if (tone.heat <= GraphRenderer.EMBER_FROM) return false;
    ctx.beginPath();
    ctx.arc(x, y, cell * GraphRenderer.EMBER_SIZE, 0, TWO_PI);
    ctx.globalAlpha = alpha * Math.min(0.8, 0.25 + 0.55 * tone.heat);
    if (tone.blur) {
      // On a light ground a shine only looks dirty: a tighter shadow there.
      ctx.shadowBlur = (this.lightScheme ? 2 : 5) * tone.heat;
      ctx.shadowColor = this.glowColor;
    }
    ctx.fillStyle = this.glowColor;
    ctx.fill();
    return true;
  }
  /** How fast a field breathes on the map (radians per second) — slower than the hive's 1.7, a map is read. */
  private static readonly MAP_BREATH = 0.8;
  /** How fast the metal's light wanders across the map (radians per second). */
  private static readonly MAP_SWEEP = 0.16;

  /** Whether a note exists at the moment the time travel shows (always, when it shows today). */
  private bornBy(n: GraphNode): boolean {
    const cutoff = this.timeCutoff;
    if (cutoff === null || !n.created) return true;
    const born = Date.parse(n.created);
    return Number.isNaN(born) || born <= cutoff;
  }

  /**
   * The notes the layout draws one by one — the map's fields (those placed on its grid), or every note for the
   * honeycomb and the bundle's ring — in a list kept for as long as the model, so their tones are worked out once a
   * second.
   */
  private cellNotes(model: GraphModel): GraphNode[] {
    // Keyed by the mode too: a frame may fall between a layout switch and the new model it brings.
    const mode = this.options.mode;
    if (this.cellFiles?.model !== model || this.cellFiles.mode !== mode) {
      const files = model.nodes.filter((n) => n.kind === "file" && (mode !== "map" || n.cell !== undefined));
      this.cellFiles = { model, mode, files };
    }
    return this.cellFiles.files;
  }

  /** Where the metal's light stands this frame for the map and the honeycomb: it wanders slowly across the lens; at
   *  Calm and Off it stands at the upper left (G3). */
  private cellLight(t: number): CellLight {
    const { width, height } = this;
    const still = this.options.pulse === false;
    const lens = Math.min(width, height) * this.options.fit;
    const sweep = t * GraphRenderer.MAP_SWEEP;
    return {
      still,
      shine: this.options.shine ?? "metal",
      x: width / 2 + (still ? -0.45 : Math.cos(sweep) * 0.6) * lens,
      y: height / 2 + (still ? -0.4 : Math.sin(sweep) * 0.45) * lens,
      spread: 2 * (lens * 0.4) ** 2,
    };
  }

  /** A flat cell's fill under the shimmer (G7) at (`x`, `y`) px: it breathes, and under "metal" it catches the light
   *  where `light` stands. Scaling toward black and mixing toward white both keep the note's hue. */
  private shimmerFill(col: Rgb, x: number, y: number, phase: number, t: number, light: CellLight): string {
    let [r, g, b] = col;
    if (light.shine !== "calm") {
      const wave = light.still ? 1 : 0.82 + 0.18 * Math.sin(t * GraphRenderer.MAP_BREATH + phase * TWO_PI);
      let lift = 0;
      if (light.shine === "metal") {
        const catchL = Math.exp(-((x - light.x) ** 2 + (y - light.y) ** 2) / light.spread);
        lift = catchL * (light.still ? 0.8 : 0.5 + 0.5 * wave) * 0.4;
      }
      r = Math.min(255, r * wave + (255 - r) * lift);
      g = Math.min(255, g * wave + (255 - g) * lift);
      b = Math.min(255, b * wave + (255 - b) * lift);
    }
    return `rgb(${Math.round(r)},${Math.round(g)},${Math.round(b)})`;
  }

  /** One honeycomb cell: a filled flat-top hexagon (matches `layoutHex`'s axial→pixel orientation) with a thin
   *  separating stroke, an ember in the middle while its note is fresh, outlined when hovered (text colour) or
   *  selected (the glow). */
  private drawHexCell(
    x: number,
    y: number,
    r: number,
    fill: string,
    alpha: number,
    hot: "selected" | "hover" | null,
    tone: HiveTone,
  ): void {
    const { ctx } = this;
    ctx.beginPath();
    for (let k = 0; k < 6; k++) {
      const a = (Math.PI / 3) * k;
      ctx.lineTo(x + Math.cos(a) * r, y + Math.sin(a) * r);
    }
    ctx.closePath();
    ctx.globalAlpha = alpha;
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.globalAlpha = hot ? 1 : Math.min(1, alpha + 0.3);
    ctx.strokeStyle = hot === "selected" ? this.glowColor : hot === "hover" ? this.textColor : this.surfaceColor;
    ctx.lineWidth = hot ? 1.8 : 1;
    ctx.stroke();
    if (this.drawEmber(ctx, x, y, r, alpha, tone)) {
      ctx.shadowBlur = 0;
    }
    ctx.globalAlpha = 1;
  }

  /** A skill's px radius on the ring around the map: steady while zooming, so the ring reads as a frame. */
  private mapSkillPx(): number {
    const lens = Math.min(this.width, this.height) * this.options.fit;
    return Math.max(9, Math.min(16, lens * 0.03));
  }

  /** The map: what is under a CSS-px point — a country's name first (drawn on top), then a skill, then a field. */
  private hitMap(px: number, py: number, slop: number): GraphNode | null {
    const model = this.model;
    if (!model) return null;
    for (const box of this.mapLabels) {
      if (Math.abs(px - box.x) <= box.w / 2 && Math.abs(py - box.y) <= box.h / 2) return box.node;
    }
    const skill = this.mapSkillPx() + slop;
    for (const n of model.nodes) {
      if (n.kind !== "skill") continue;
      const p = this.toScreen(n);
      if (Math.hypot(p.x - px, p.y - py) < skill) return n;
    }
    // The field is found the way it is drawn: the point taken back to the axial grid and rounded to its hex — exact at
    // every edge, and one lookup instead of a scan.
    const unit = model.hexUnit;
    const centre = model.mapCentre;
    if (!unit || !centre) return null;
    const R = this.radius();
    const gx = (px - this.width / 2 - this.view.x) / R;
    const gy = (py - this.height / 2 - this.view.y) / R;
    const [q, r] = pixelToAxial(gx / unit + centre.x, gy / unit + centre.y);
    const field = model.territory?.get(cellKey(q, r)) ?? null;
    return field && this.bornBy(field) ? field : null;
  }

  /**
   * The territory map (UX5, G16): every note a field in its country's colour — glowing where it was touched today,
   * darker where nothing moved for weeks, lit by the shimmer (G7) — with a border where countries meet or
   * the land ends. The zoom decides what else there is (semantic zoom): from afar the countries' names, closer the
   * links as roads, close up every field's name. Skills ring the map. In the time travel a note not yet born has no
   * field: the countries grow.
   */
  private drawMap(t: number): void {
    const { ctx, width, height } = this;
    const model = this.model!;
    const notes = this.cellNotes(model);
    const tones = this.noteTones(notes, GraphRenderer.MAP_COOLING);
    const s = this.hexCellRadius();
    const zoom = this.view.zoom;
    const hl = this.highlight;
    const light = this.cellLight(t);
    const shine = light.shine;
    const onScreen = (x: number, y: number) => x > -s && x < width + s && y > -s && y < height + s;

    // Fields.
    const middles = new Map<string, { x: number; y: number; n: number }>();
    const shown = new Set<GraphNode>();
    notes.forEach((n, i) => {
      const tone = tones[i];
      if (!tone?.settled) return;
      shown.add(n);
      // The field's screen position, worked out once per frame and kept on the node for the passes below.
      const p = this.toScreen(n);
      n.sx = p.x;
      n.sy = p.y;
      if (n.area) {
        const m = middles.get(n.area) ?? { x: 0, y: 0, n: 0 };
        m.x += p.x;
        m.y += p.y;
        m.n++;
        middles.set(n.area, m);
      }
      if (!onScreen(p.x, p.y)) return;
      const fill = this.shimmerFill(tone.col, p.x, p.y, n.phase, t, light);
      ctx.save();
      ctx.translate(p.x, p.y);
      this.hexagonPath(s * 0.94);
      const alpha = hl !== null && !hl.has(n.id) && n !== this.selected ? 0.15 : 1;
      ctx.globalAlpha = alpha;
      ctx.fillStyle = fill;
      ctx.fill();
      // A fresh note's glow: an ember in the middle of its field, as bright as the note is fresh.
      if (this.drawEmber(ctx, 0, 0, s, alpha, tone)) ctx.globalAlpha = alpha;
      if (shine === "metal") {
        // A plate's lit upper rim — upper-left corner, top, upper-right corner (y points down): what makes the
        // fields read as tilted metal.
        const rim = s * 0.94;
        const side = Math.cos(Math.PI / 6) * rim;
        ctx.shadowBlur = 0;
        ctx.beginPath();
        ctx.moveTo(-side, -rim / 2);
        ctx.lineTo(0, -rim);
        ctx.lineTo(side, -rim / 2);
        ctx.strokeStyle = this.lightScheme ? "rgba(255,255,255,0.55)" : "rgba(255,255,255,0.14)";
        ctx.lineWidth = 1;
        ctx.stroke();
      }
      ctx.restore();
    });
    ctx.globalAlpha = 1;

    // Borders: an edge whose neighbour is another country, or no land at all, is drawn in the field's own colour —
    // where two countries meet, both colours run side by side. One path per field; the corners are worked out once.
    const k = s * 0.9;
    const edgeEnds = NEIGHBOURS.map(({ angle }) => {
      const a = (angle * Math.PI) / 180;
      return [
        Math.cos(a - Math.PI / 6) * k,
        Math.sin(a - Math.PI / 6) * k,
        Math.cos(a + Math.PI / 6) * k,
        Math.sin(a + Math.PI / 6) * k,
      ] as const;
    });
    ctx.lineWidth = Math.max(1.2, 1.4 * Math.sqrt(zoom));
    ctx.lineCap = "round";
    ctx.globalAlpha = 0.9;
    for (const n of shown) {
      const x = n.sx ?? 0;
      const y = n.sy ?? 0;
      if (!onScreen(x, y)) continue;
      let any = false;
      ctx.beginPath();
      edgeEnds.forEach(([x1, y1, x2, y2], i) => {
        const nb = n.around?.[i] ?? null;
        if (nb && nb.area === n.area && shown.has(nb)) return;
        ctx.moveTo(x + x1, y + y1);
        ctx.lineTo(x + x2, y + y2);
        any = true;
      });
      if (!any) continue;
      ctx.strokeStyle = n.color;
      ctx.stroke();
    }
    ctx.globalAlpha = 1;

    // Roads: the links between notes, from mid zoom on — a hovered or selected note's always. Below that zoom only the
    // links of those two are looked at.
    const roads = Math.min(1, Math.max(0, (zoom - GraphRenderer.ROADS_FROM) * 2));
    const hotIds = [this.hover?.id, this.selected?.id];
    ctx.lineWidth = 1;
    for (const e of model.edges) {
      const hot = hotIds.includes(e.from) || hotIds.includes(e.to);
      if (!hot && roads === 0) continue;
      const a = model.byId.get(e.from);
      const b = model.byId.get(e.to);
      if (a?.kind !== "file" || b?.kind !== "file" || !shown.has(a) || !shown.has(b)) continue;
      const ax = a.sx ?? 0;
      const ay = a.sy ?? 0;
      const bx = b.sx ?? 0;
      const by = b.sy ?? 0;
      if (!hot && !onScreen(ax, ay) && !onScreen(bx, by)) continue;
      ctx.setLineDash(hot ? [] : [3, 4]);
      ctx.strokeStyle = hot ? this.glowColor : this.textColor;
      ctx.globalAlpha = hot ? 0.9 : 0.2 * roads;
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      // Bowed a little, so roads that share a field do not run on top of each other.
      ctx.quadraticCurveTo((ax + bx) / 2 + (by - ay) * 0.15, (ay + by) / 2 - (bx - ax) * 0.15, bx, by);
      ctx.stroke();
    }
    ctx.setLineDash([]);
    ctx.globalAlpha = 1;

    // Names on the fields, close up (or always, with the file names switched on).
    const names = zoom >= GraphRenderer.NAMES_FROM || this.options.fileLabels;
    if (names && s >= 14) {
      ctx.font = this.font(Math.min(GraphRenderer.FIELD_NAME_MAX_PX, s * 0.3), 500);
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillStyle = this.textColor;
      for (const n of shown) {
        const x = n.sx ?? 0;
        const y = n.sy ?? 0;
        if (!onScreen(x, y)) continue;
        ctx.globalAlpha = hl !== null && !hl.has(n.id) ? 0.25 : 0.92;
        ctx.fillText(this.fitText(n.label, s * 1.5), x, y);
      }
      ctx.globalAlpha = 1;
    }

    // The countries' names, from afar.
    this.mapLabels = [];
    const countries = Math.min(1, Math.max(0, (GraphRenderer.COUNTRIES_UNTIL - zoom) * 1.5));
    if (this.options.labels && countries > 0) {
      ctx.globalAlpha = countries;
      for (const area of model.nodes) {
        if (area.kind !== "area" || !area.area) continue;
        const m = middles.get(area.area);
        if (!m) continue;
        const x = m.x / m.n;
        const y = m.y / m.n;
        const size = 13 + 2 * zoom;
        const w = this.haloText(area.label, x, y, this.font(size, 650), this.textColor);
        const count = `${m.n} ${m.n === 1 ? "note" : "notes"}`;
        this.haloText(count, x, y + size * 0.6 + 8, this.font(11.5, 500), this.mutedColor);
        if (countries > 0.5) this.mapLabels.push({ node: area, x, y: y + 6, w: w + 12, h: size + 26 });
      }
      ctx.globalAlpha = 1;
    }

    // The hovered and the selected field: outlined, and named above it — over the countries' names, and only while the
    // fields do not carry their names themselves.
    for (const n of [this.selected, this.hover]) {
      if (!n || n.kind !== "file" || !shown.has(n)) continue;
      const p = { x: n.sx ?? 0, y: n.sy ?? 0 };
      ctx.save();
      ctx.translate(p.x, p.y);
      this.hexagonPath(s * 1.04);
      ctx.strokeStyle = n === this.selected ? this.glowColor : this.textColor;
      ctx.lineWidth = 2;
      ctx.stroke();
      ctx.restore();
      if (!names) this.haloText(n.label, p.x, p.y - s - 8, this.font(12, 600), this.textColor);
    }

    // Skills ring the map.
    const skillR = this.mapSkillPx();
    for (const n of model.nodes) {
      if (n.kind !== "skill") continue;
      const p = this.toScreen(n);
      const hot = n === this.hover || n === this.selected;
      this.drawNodeDisc(p.x, p.y, skillR, n.color, hot);
      drawGlyph(ctx, glyphOf(n), p.x, p.y, skillR * 0.62, hot ? this.accentColor : n.color);
      if (hot) this.drawHotLabel(p.x, p.y, skillR, n.label, 0.75, 0.6);
    }

    this.drawPulse();
  }

  /** How far an old note darkens on the bundle's ring: a dot is small, so it keeps more of its area's light. */
  private static readonly BUNDLE_COOLING = 0.6;
  /** A calm link's strength, a new one's (in the glow), and a link's while another note's are being looked at. */
  private static readonly LINK_ALPHA = 0.22;
  private static readonly FRESH_LINK_ALPHA = 0.6;
  private static readonly DIMMED_LINK_ALPHA = 0.05;

  /** The bundle's link curves for the current model, worked out once per model (`graph/bundle.ts`). */
  private bundleLinks: { model: GraphModel; links: BundleLink[] } | null = null;
  /** What the last bundle frame drew where — notes, area icons, skills — for the hit test. */
  private bundleHits: { node: GraphNode; x: number; y: number; r: number }[] = [];

  /** Every link between two notes with its curve along the vault's tree and its birth (`linkBorn`). */
  private linksOf(model: GraphModel): BundleLink[] {
    if (this.bundleLinks?.model === model) return this.bundleLinks.links;
    const layout = model.bundle;
    const links: BundleLink[] = [];
    if (layout) {
      for (const e of model.edges) {
        const a = model.byId.get(e.from);
        const b = model.byId.get(e.to);
        if (a?.kind !== "file" || b?.kind !== "file" || a === b) continue;
        const path = bundleCurve(layout, a.id, b.id, BUNDLE_BETA);
        if (path.length > 0) links.push({ a, b, born: linkBorn(bornOf(a), bornOf(b)), path });
      }
    }
    this.bundleLinks = { model, links };
    return links;
  }

  /** Adds a curve in graph units to the current path, on a ring of `R` px around (`cx`, `cy`). */
  private tracePath(path: readonly BundleCommand[], cx: number, cy: number, R: number): void {
    const { ctx } = this;
    for (const c of path) {
      if (c.kind === "move") ctx.moveTo(cx + c.x * R, cy + c.y * R);
      else if (c.kind === "line") ctx.lineTo(cx + c.x * R, cy + c.y * R);
      else ctx.bezierCurveTo(cx + c.x1 * R, cy + c.y1 * R, cx + c.x2 * R, cy + c.y2 * R, cx + c.x * R, cy + c.y * R);
    }
  }

  /** The bundle: the nearest of the notes, area icons and skills drawn in the last frame, within reach. */
  private hitBundle(px: number, py: number, slop: number): GraphNode | null {
    let best: GraphNode | null = null;
    let bestD = Infinity;
    for (const h of this.bundleHits) {
      const d = Math.hypot(h.x - px, h.y - py);
      if (d < h.r + slop && d < bestD) {
        best = h.node;
        bestD = d;
      }
    }
    return best;
  }

  /**
   * The Bundle layout (`docs/plans/brain-views.md`, BV1): the notes on one ring, an arc per area with the area's
   * colour as a band outside it, its icon and name further out, the skills on a ring beyond. The links run through the
   * middle as curves bundled along the vault's tree, in the colour of the note they start from, faint — the links of
   * the last weeks in the glow (Q11). Looking at a note (or an area) lifts its links and dims the rest. In the time
   * travel a note not yet born and its links are not there (Q3).
   */
  private drawBundle(_t: number): void {
    const { ctx, width, height } = this;
    const model = this.model!;
    const layout = model.bundle;
    if (!layout) return;
    const R = this.radius();
    const zoom = this.view.zoom;
    const cx = width / 2 + this.view.x;
    const cy = height / 2 + this.view.y;
    const notes = this.cellNotes(model);
    const tones = this.noteTones(notes, GraphRenderer.BUNDLE_COOLING);
    const shown = new Set<GraphNode>();
    notes.forEach((n, i) => {
      if (tones[i]?.settled) shown.add(n);
    });
    const now = this.timeCutoff ?? Date.now();
    const focus = this.hover ?? this.selected;
    const touches = (n: GraphNode) =>
      focus !== null && (focus.kind === "area" ? n.area === focus.area : focus.kind === "file" && n === focus);
    const hl = this.highlight;
    const hits: typeof this.bundleHits = [];
    const areaColor = new Map(model.nodes.filter((n) => n.kind === "area").map((n) => [n.area, n.color]));

    // The areas' band, just outside their notes.
    ctx.lineWidth = Math.max(2, 3 * Math.sqrt(zoom));
    ctx.lineCap = "butt";
    for (const arc of layout.arcs) {
      if (arc.end <= arc.start) continue;
      ctx.strokeStyle = (arc.area !== null ? areaColor.get(arc.area) : undefined) ?? this.mutedColor;
      ctx.globalAlpha = focus?.kind === "area" && focus.area !== arc.area ? 0.25 : 0.75;
      ctx.beginPath();
      ctx.arc(cx, cy, R * BUNDLE.band, arc.start, arc.end);
      ctx.stroke();
    }

    // The links: calm ones grouped by colour into one path each, the looked-at ones on top.
    const calm = new Map<string, { color: string; fresh: boolean; links: BundleLink[] }>();
    const hot: BundleLink[] = [];
    const hotNotes = new Set<GraphNode>();
    for (const l of this.linksOf(model)) {
      if (!shown.has(l.a) || !shown.has(l.b)) continue;
      if (touches(l.a) || touches(l.b)) {
        hot.push(l);
        hotNotes.add(l.a);
        hotNotes.add(l.b);
        continue;
      }
      const fresh = isFreshLink(l.born, now);
      const color = fresh ? this.glowColor : l.a.color;
      const key = `${fresh ? 1 : 0}${color}`;
      const group = calm.get(key) ?? { color, fresh, links: [] };
      group.links.push(l);
      calm.set(key, group);
    }
    ctx.save();
    // On a dark ground the bundles add up to light where many run together; on a light one that would wash out.
    if (!this.lightScheme) ctx.globalCompositeOperation = "screen";
    ctx.lineWidth = 1;
    for (const group of calm.values()) {
      ctx.strokeStyle = group.color;
      ctx.globalAlpha =
        focus !== null || hl !== null
          ? GraphRenderer.DIMMED_LINK_ALPHA
          : group.fresh
            ? GraphRenderer.FRESH_LINK_ALPHA
            : GraphRenderer.LINK_ALPHA;
      ctx.beginPath();
      for (const l of group.links) this.tracePath(l.path, cx, cy, R);
      ctx.stroke();
    }
    ctx.lineWidth = 1.7;
    ctx.globalAlpha = 0.95;
    for (const l of hot) {
      ctx.strokeStyle = isFreshLink(l.born, now) ? this.glowColor : l.a.color;
      ctx.beginPath();
      this.tracePath(l.path, cx, cy, R);
      ctx.stroke();
    }
    ctx.restore();

    // The notes: a dot each in its tone, an ember while it is fresh.
    const spacing = (TWO_PI * BUNDLE.notes * R) / Math.max(1, notes.length);
    const dotCap = Math.max(1.2, spacing * 0.42);
    const dotOf = (n: GraphNode) => Math.min(dotCap, Math.max(1.2, n.r * Math.sqrt(zoom)));
    notes.forEach((n, i) => {
      const tone = tones[i];
      if (!tone?.settled) return;
      const p = this.toScreen(n);
      const r = dotOf(n);
      const dimmed = (focus !== null && !touches(n) && !hotNotes.has(n)) || (hl !== null && !hl.has(n.id));
      const alpha = dimmed ? 0.3 : 1;
      ctx.globalAlpha = alpha;
      ctx.fillStyle = `rgb(${Math.round(tone.col[0])},${Math.round(tone.col[1])},${Math.round(tone.col[2])})`;
      ctx.beginPath();
      ctx.arc(p.x, p.y, r, 0, TWO_PI);
      ctx.fill();
      // The ember's size is a share of its cell's; a dot's ember is about half the dot.
      if (this.drawEmber(ctx, p.x, p.y, Math.max(1, r * 0.5) / GraphRenderer.EMBER_SIZE, alpha, tone)) {
        ctx.shadowBlur = 0;
      }
      if (n === this.selected || n === this.hover) {
        ctx.globalAlpha = 1;
        ctx.strokeStyle = n === this.selected ? this.glowColor : this.textColor;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.arc(p.x, p.y, r + 3, 0, TWO_PI);
        ctx.stroke();
      }
      hits.push({ node: n, x: p.x, y: p.y, r: Math.max(r, 3) });
    });
    ctx.globalAlpha = 1;

    // File names along the radius, inside the ring, once the dots are far enough apart to carry them.
    const namePx = this.labelPx(R);
    if (this.options.fileLabels && spacing >= namePx * 1.05) {
      ctx.font = this.font(namePx, 500);
      ctx.textBaseline = "middle";
      ctx.fillStyle = this.textColor;
      for (const n of notes) {
        if (!shown.has(n)) continue;
        const a = Math.atan2(n.y, n.x);
        const p = this.toScreen(n);
        const right = Math.cos(a) >= 0;
        ctx.save();
        ctx.translate(p.x, p.y);
        ctx.rotate(right ? a : a + Math.PI);
        ctx.textAlign = right ? "right" : "left";
        ctx.globalAlpha = focus !== null && !touches(n) && !hotNotes.has(n) ? 0.3 : 0.85;
        ctx.fillText(this.fitText(n.label, R * 0.3), right ? -dotOf(n) - 5 : dotOf(n) + 5, 0);
        ctx.restore();
      }
      ctx.globalAlpha = 1;
    }

    // The areas' icons and names outside the band.
    const iconCap = this.mapSkillPx() * 0.85;
    const areaPx = Math.min(GraphRenderer.FIELD_NAME_MAX_PX, 11.5 * Math.max(1, Math.sqrt(zoom)));
    for (const arc of layout.arcs) {
      if (arc.area === null) continue;
      const node = model.byId.get(`area:${arc.area}`);
      if (!node) continue;
      const mid = (arc.start + arc.end) / 2;
      const iconR = Math.min(iconCap, (arc.end - arc.start) * BUNDLE.areas * R * 0.45);
      const hot = node === this.hover || node === this.selected;
      ctx.globalAlpha = focus?.kind === "area" && !hot ? 0.45 : 1;
      if (iconR >= 5) {
        const p = this.toScreen(node);
        this.drawNodeDisc(p.x, p.y, iconR, node.color, hot);
        drawGlyph(ctx, glyphOf(node), p.x, p.y, iconR * 0.62, hot ? this.accentColor : node.color);
        hits.push({ node, x: p.x, y: p.y, r: iconR });
      }
      if (!this.options.labels) continue;
      // Along the ring, upright on either half; cut to the room its arc leaves.
      const font = this.font(areaPx, 600);
      ctx.font = font;
      // A name may reach a little past its arc, into the gaps on either side.
      const room = (arc.end - arc.start) * BUNDLE.names * R + 20;
      const text = this.fitText(node.label, room);
      // Nothing left but a letter and the ellipsis says nothing: the icon (and the detail panel) name it then.
      if (text.endsWith("…") && text.length < 4) continue;
      const x = cx + Math.cos(mid) * BUNDLE.names * R;
      const y = cy + Math.sin(mid) * BUNDLE.names * R;
      ctx.save();
      ctx.translate(x, y);
      ctx.rotate(Math.sin(mid) <= 0 ? mid + Math.PI / 2 : mid - Math.PI / 2);
      this.haloText(text, 0, 0, font, node.color);
      ctx.restore();
    }
    ctx.globalAlpha = 1;

    // The skills' ring.
    const skillR = this.mapSkillPx();
    for (const n of model.nodes) {
      if (n.kind !== "skill") continue;
      const p = this.toScreen(n);
      const hot = n === this.hover || n === this.selected;
      this.drawNodeDisc(p.x, p.y, skillR, n.color, hot);
      drawGlyph(ctx, glyphOf(n), p.x, p.y, skillR * 0.62, hot ? this.accentColor : n.color);
      if (hot) this.drawHotLabel(p.x, p.y, skillR, n.label, 0.75, 0.6);
      hits.push({ node: n, x: p.x, y: p.y, r: skillR });
    }

    // The looked-at note's name, inside the ring beside its dot.
    for (const n of [this.selected, this.hover]) {
      if (!n || n.kind !== "file" || !shown.has(n)) continue;
      const a = Math.atan2(n.y, n.x);
      const p = this.toScreen(n);
      const inward = dotOf(n) + 10;
      const font = this.font(Math.max(12, namePx), 600);
      if (Math.abs(Math.cos(a)) < 0.35) {
        this.haloText(n.label, p.x, p.y - Math.sign(Math.sin(a)) * (inward + 4), font, this.textColor);
      } else {
        const right = Math.cos(a) > 0;
        this.haloText(n.label, p.x - Math.cos(a) * inward, p.y - Math.sin(a) * inward, font, this.textColor, right ? "right" : "left");
      }
    }

    this.bundleHits = hits;
    this.drawPulse();
  }

  /** `text` cut to `max` px with an ellipsis, in the current font. */
  private fitText(text: string, max: number): string {
    const { ctx } = this;
    if (ctx.measureText(text).width <= max) return text;
    let cut = text;
    while (cut.length > 1 && ctx.measureText(`${cut}…`).width > max) cut = cut.slice(0, -1);
    return `${cut}…`;
  }

  /** Text on a halo of the ground colour, so it reads over the fields; returns its width. */
  private haloText(
    text: string,
    x: number,
    y: number,
    font: string,
    color: string,
    align: CanvasTextAlign = "center",
  ): number {
    const { ctx } = this;
    ctx.font = font;
    ctx.textAlign = align;
    ctx.textBaseline = "middle";
    ctx.lineJoin = "round";
    ctx.lineWidth = 4;
    ctx.strokeStyle = this.surfaceColor;
    ctx.strokeText(text, x, y);
    ctx.fillStyle = color;
    ctx.fillText(text, x, y);
    return ctx.measureText(text).width;
  }

  private badgeFor(n: GraphNode, _t: number): string | null {
    if (n.kind === "file" && n.modified) {
      const days = Math.floor((Date.now() - Date.parse(n.modified)) / 86_400_000);
      if (days < 1) return "NEW";
      return days < 30 ? `${days}D` : days < 365 ? `${Math.floor(days / 30)}M` : `${Math.floor(days / 365)}Y`;
    }
    return null;
  }
}

/** The App Ring's icon-node radius in px, given the orbit ring's own radius
 *  `R`. Exported (unlike the rim loop's own, differently-scaled `nodeR`)
 *  so `second-brain.svelte` can size and place its "+" DOM button to match
 *  the canvas-drawn app nodes exactly, instead of a fixed CSS size that
 *  would drift from the ring's actual scale on resize — or simply read as
 *  a separate piece of chrome rather than a slot on the ring itself. */
export function appNodeRadiusPx(R: number): number {
  return Math.max(14, Math.min(26, R * 0.075));
}

/** The orbit ring's own icon-node radius in px, given the ring's radius `R`
 *  and how many icon nodes (`onOrbit: true` — skills, recent
 *  files) currently share its circumference.
 *
 *  A plain `R`-only formula assumed a roughly-constant node count; once
 *  `ORBIT_MAX` (the ring's own cap, `graph/layout.ts`) could actually be
 *  reached in practice, that assumption broke — neighbouring icons started
 *  overlapping on any vault with enough recently-modified files to fill the
 *  ring, which is the normal populated state, not an edge case. Capping by
 *  the arc length actually available per node fixes that without changing
 *  anything about a lightly-populated ring, where the size-based cap below
 *  is still the smaller (hence binding) one. Exported so the fix is
 *  unit-testable without a canvas. */
export function rimNodeRadiusPx(R: number, onOrbitCount: number): number {
  // Owner feedback (2026-09-13, two rounds): the App Ring (outside this
  // rim, see `appNodeRadiusPx`) reads well as-is; this inner rim's own icon
  // nodes (skills/recent files on the orbit sphere's own surface)
  // kept reading too big next to it, so its size cap sits further below the
  // App Ring's own `Math.max(14, Math.min(26, R * 0.075))` than the first
  // pass (`Math.max(14, Math.min(26, R * 0.08))`).
  const sizeCap = Math.max(12, Math.min(22, R * 0.07));
  // `* 0.4` (not `0.5`) leaves a visible gap between adjacent icons instead
  // of just touching edge-to-edge at the crowding limit.
  const spacingCap = onOrbitCount > 0 ? ((TWO_PI * R) / onOrbitCount) * 0.4 : sizeCap;
  return Math.min(sizeCap, Math.max(6, spacingCap));
}

/** One or two uppercase initials from an app's display name (e.g. "Visual
 *  Studio Code" → "VS", "Slack" → "S") — the App Ring's stand-in icon for a
 *  user-added Mac app, which has no extracted `.app` icon in v1. */
function monogram(label: string): string {
  const words = label.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  const first = words[0]?.[0] ?? "";
  const second = words.length > 1 ? (words[1]?.[0] ?? "") : "";
  return (first + second).toUpperCase();
}

function scale(v: [number, number, number], k: number): [number, number, number] {
  return [v[0] * k, v[1] * k, v[2] * k];
}

