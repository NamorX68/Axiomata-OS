/**
 * The graph as the renderer sees it: flat node list with kind / colour /
 * position, edges by node id, and the area ring segments. Built from the
 * Rust `WorkspaceGraph` payload plus a palette read from the active theme.
 */

import type { WorkspaceGraph } from "../core/backend";

export type NodeKind = "hub" | "area" | "file" | "skill" | "routine";

export interface GraphNode {
  id: string;
  kind: NodeKind;
  label: string;
  area: string | null;
  /** Workspace-relative path for files / hub. */
  path?: string;
  bytes: number;
  enabled?: boolean;
  /** Layout position in graph units (0,0 = centre). */
  x: number;
  y: number;
  /** Base radius in px at zoom 1. */
  r: number;
  color: string;
  /** Twinkle phase offset. */
  phase: number;
  /** Number of links touching the node. */
  degree: number;
  modified?: string | null;
  isMarkdown?: boolean;
  /** Area nodes only: which icon to draw (see `glyphForArea`); other kinds
   *  are keyed by `kind` itself ("hub" / "skill" / "routine"). */
  glyph?: string;
  /** Orbit mode: 3-D point of the particle cloud (graph units). */
  p3?: [number, number, number];
  /** Orbit mode: node sits on the icon ring. */
  onOrbit?: boolean;
  /** Orbit mode: last projected screen position (for hit-testing). */
  sx?: number;
  sy?: number;
}

export interface GraphEdge {
  from: string;
  to: string;
}

export interface AreaSegment {
  name: string;
  color: string;
  count: number;
  /** Angular range in radians. */
  start: number;
  end: number;
}

export interface GraphModel {
  nodes: GraphNode[];
  edges: GraphEdge[];
  areas: AreaSegment[];
  byId: Map<string, GraphNode>;
  totalFiles: number;
  truncated: boolean;
}

export interface Palette {
  text: string;
  muted: string;
  accent: string;
  warning: string;
  success: string;
  border: string;
  /** Contrast colour for glyphs on coloured nodes (`--ax-text-invert`). */
  invert: string;
  /** Tile surface (`--ax-surface-1`), used for orbit-node fills. */
  surface: string;
  light: boolean;
}

/** Reads the palette from the `--ax-*` tokens currently in effect. */
export function readPalette(): Palette {
  const cs = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback;
  return {
    text: v("--ax-text", "#f5f5f7"),
    muted: v("--ax-text-muted", "#94949c"),
    accent: v("--ax-accent", "#ff7a1a"),
    warning: v("--ax-warning", "#e6b45f"),
    success: v("--ax-success", "#4fd67f"),
    border: v("--ax-border-strong", "#3b3b43"),
    invert: v("--ax-text-invert", "#0b0b0d"),
    surface: v("--ax-surface-1", "#121216"),
    light: v("--ax-color-scheme", "dark") === "light",
  };
}

/**
 * Which icon an area (folder) node draws, guessed from its own name — the
 * same idea as the lightning bolt on a skill, but per-folder instead of one
 * shape for every area. Checked in order, first match wins, against the
 * lower-cased full path (so a nested folder like "Learning/Rust/lessons"
 * still matches "learning" even though its short label is "Rust/lessons");
 * falls back to a plain folder glyph for anything unmatched. Deliberately a
 * short curated list, not a giant keyword dictionary — new areas just get
 * the folder default until a rule is worth adding.
 */
const AREA_GLYPH_RULES: [RegExp, string][] = [
  [/\brust\b|\bblockos\b|entwicklung|\bdev\b|programm(ier)?/i, "code"],
  [/\bki\b|\bai\b|intelligenz|machine.?learning/i, "chip"],
  [/lernen|learning|\bkurs(e)?\b|\bcourse/i, "book"],
  [/arbeit|\bwork\b|\bjob\b/i, "briefcase"],
  [/foto|photo|kamera|camera/i, "camera"],
  [/gesellschaft|society|politik|kultur/i, "people"],
  [/pers[oö]nlich|personal|privat/i, "user"],
  [/system|werkzeug|\btools?\b/i, "wrench"],
  [/inbox|eingang/i, "tray"],
];

export function glyphForArea(path: string): string {
  for (const [pattern, glyph] of AREA_GLYPH_RULES) {
    if (pattern.test(path)) return glyph;
  }
  return "folder";
}

/** Stable per-area hue from the name; saturation/lightness by scheme. */
export function areaColor(name: string, light: boolean): string {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  const hue = (h % 12) * 30 + 200; // spread across the wheel, offset from the orange accent
  return light ? `hsl(${hue % 360} 55% 42%)` : `hsl(${hue % 360} 70% 68%)`;
}

const AREA_GAP = 0.06; // radians between segments
/** Free angle around 12 o'clock for the ring captions. */
export const CAPTION_GAP = 0.26;

export type Grouping = "areas" | "folders";

/** Re-keys every file's area to its full parent folder (for the "folders"
 *  view); the root stays `null`. Areas are recomputed from the files. */
export function regroup(g: WorkspaceGraph, grouping: Grouping): WorkspaceGraph {
  if (grouping === "areas") return g;
  const counts = new Map<string, number>();
  const files = g.files.map((f) => {
    const dir = f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : null;
    if (dir) counts.set(dir, (counts.get(dir) ?? 0) + 1);
    return { ...f, area: dir };
  });
  return {
    ...g,
    files,
    areas: [...counts.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([name, n]) => ({ name, files: n })),
  };
}

/** Node ids whose label / path / area contain every word of `query`. */
export function searchNodes(model: GraphModel, query: string): Set<string> {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const hits = new Set<string>();
  if (words.length === 0) return hits;
  for (const n of model.nodes) {
    const hay = `${n.label} ${n.path ?? ""} ${n.area ?? ""}`.toLowerCase();
    if (words.every((w) => hay.includes(w))) hits.add(n.id);
  }
  return hits;
}

/** Neighbours of a node via the edge list, as `{ id, direction }`. */
export function neighbours(model: GraphModel, id: string): { node: GraphNode; out: boolean }[] {
  const out: { node: GraphNode; out: boolean }[] = [];
  for (const e of model.edges) {
    if (e.from === id) {
      const n = model.byId.get(e.to);
      if (n) out.push({ node: n, out: true });
    } else if (e.to === id) {
      const n = model.byId.get(e.from);
      if (n) out.push({ node: n, out: false });
    }
  }
  return out;
}

export function buildModel(g: WorkspaceGraph, palette: Palette): GraphModel {
  const nodes: GraphNode[] = [];
  const byId = new Map<string, GraphNode>();
  const add = (n: GraphNode) => {
    nodes.push(n);
    byId.set(n.id, n);
  };
  const phase = (s: string) => {
    let h = 0;
    for (const ch of s) h = (h * 33 + ch.charCodeAt(0)) >>> 0;
    return (h % 1000) / 1000;
  };

  add({
    id: "hub",
    kind: "hub",
    label: g.hub ?? "CLAUDE.md",
    area: null,
    path: g.hub ?? undefined,
    bytes: 0,
    x: 0,
    y: 0,
    r: 13,
    color: palette.text,
    phase: 0,
    degree: 0,
  });

  for (const s of g.skills) {
    add({
      id: `skill:${s.name}`,
      kind: "skill",
      label: `/${s.name}`,
      area: null,
      bytes: 0,
      x: 0,
      y: 0,
      r: 9,
      color: palette.accent,
      phase: phase(s.name),
      degree: 0,
    });
  }

  // Area segments proportional to file count.
  const counted = g.areas.filter((a) => a.files > 0);
  const total = counted.reduce((n, a) => n + a.files, 0) || 1;
  const usable = Math.PI * 2 - AREA_GAP * counted.length;
  // Start a little past 12 o'clock so the ring captions up there stay clear.
  let angle = -Math.PI / 2 + CAPTION_GAP;
  const areas: AreaSegment[] = counted.map((a) => {
    const span = (a.files / total) * usable;
    const seg = { name: a.name, color: areaColor(a.name, palette.light), count: a.files, start: angle, end: angle + span };
    angle += span + AREA_GAP;
    return seg;
  });
  const colorOf = new Map(areas.map((a) => [a.name, a.color]));

  // One node per area (the folder level under CLAUDE.md), named like the
  // folder; files hang off it, it hangs off the hub.
  for (const a of areas) {
    add({
      id: `area:${a.name}`,
      kind: "area",
      label: a.name.split("/").slice(-2).join("/"),
      area: a.name,
      bytes: a.count,
      x: 0,
      y: 0,
      r: 8,
      color: a.color,
      phase: phase(a.name),
      degree: 0,
      glyph: glyphForArea(a.name),
    });
  }

  for (const f of g.files) {
    add({
      id: `file:${f.path}`,
      kind: "file",
      label: f.title,
      area: f.area,
      path: f.path,
      bytes: f.bytes,
      x: 0,
      y: 0,
      r: 1.6 + Math.min(2.4, Math.log10(1 + f.bytes) * 0.5),
      color: f.area ? (colorOf.get(f.area) ?? palette.muted) : palette.muted,
      phase: phase(f.path),
      degree: 0,
      modified: f.modified,
      isMarkdown: f.is_markdown,
    });
  }

  for (const r of g.routines) {
    add({
      id: `routine:${r.id}`,
      kind: "routine",
      label: r.name,
      area: null,
      bytes: 0,
      enabled: r.enabled,
      x: 0,
      y: 0,
      r: 8,
      color: palette.warning,
      phase: phase(r.name),
      degree: 0,
    });
  }

  const edges: GraphEdge[] = [];
  for (const l of g.links) {
    const from = `file:${l.from}`;
    const to = `file:${l.to}`;
    const a = byId.get(from);
    const b = byId.get(to);
    if (!a || !b) continue;
    edges.push({ from, to });
    a.degree++;
    b.degree++;
  }
  // Hub spokes: skills, routines and areas hang off the hub; files off
  // their area node (drawn very faintly).
  for (const n of nodes) {
    if (n.kind === "skill" || n.kind === "routine" || n.kind === "area") edges.push({ from: "hub", to: n.id });
    if (n.kind === "file" && n.area) edges.push({ from: `area:${n.area}`, to: n.id });
  }

  return { nodes, edges, areas, byId, totalFiles: g.total_files, truncated: g.truncated };
}
