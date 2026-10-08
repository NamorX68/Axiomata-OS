/**
 * The graph as the renderer sees it: flat node list with kind / colour /
 * position, edges by node id, and the area ring segments. Built from the
 * Rust `WorkspaceGraph` payload plus a palette read from the active theme.
 */

import type { AppGroup } from "../core/appGroups";
import type { BuiltinApp, UserApp } from "../core/apps";
import type { WorkspaceGraph } from "../core/backend";

export type NodeKind = "hub" | "area" | "file" | "skill" | "app";

export interface GraphNode {
  id: string;
  kind: NodeKind;
  label: string;
  area: string | null;
  /** Workspace-relative path for files / hub. */
  path?: string;
  bytes: number;
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
  /** Area nodes: which icon to draw (see `glyphForArea`). Builtin `"app"`
   *  nodes: which icon to draw (see `glyphForModuleType`) — a hand-drawn
   *  vector glyph, not a rasterized image; see that function's doc comment
   *  for why. Every other kind is keyed by `kind` itself ("hub" / "skill"). */
  glyph?: string;
  /** Orbit mode: 3-D point of the particle cloud (graph units). */
  p3?: [number, number, number];
  /** Orbit mode: node sits on the icon ring. */
  onOrbit?: boolean;
  /** Orbit mode: last projected screen position (for hit-testing). */
  sx?: number;
  sy?: number;
  /** `"app"` nodes only: `true` for an externally added Mac app (right of
   *  the ring's "+"), unset/`false` for a builtin module (left of it).
   *  Stamped fresh by `layoutAppRing` on every model rebuild — never a
   *  runtime store lookup — so both the click router and the right-click
   *  context-menu gate can read it directly off the hit node. */
  userApp?: boolean;
  /** `"app"` nodes only, builtins: the registry module `type` to pass to
   *  `createInstance`/`bringToFront`. */
  appType?: string;
  /** `"app"` nodes only, user apps: the absolute filesystem path to pass to
   *  `openPath`/`removeUserApp`. Deliberately not the shared `path` field
   *  above — that one is documented as workspace-relative (files/hub), and
   *  an absolute macOS app path would silently break that contract. */
  appPath?: string;
  /** `"app"` nodes only, App-Ring grouping: set on a group's own ring-slot
   *  node (`isGroup: true`, this is the group's own id) or on a member node
   *  currently shown on that group's expanded secondary ring (`isGroup`
   *  unset) — the two are mutually exclusive, so `core/appGroups.ts`'s
   *  `menuActionsFor` branches on `isGroup` first, `groupId` second. */
  groupId?: string;
  /** `"app"` nodes only: this node is a group's own collapsed ring-slot
   *  circle, not a solo app or an expanded member. */
  isGroup?: boolean;
  /** `"app"` nodes only: this node is a member drawn on its group's
   *  expanded secondary ring (see `layout.ts`'s `layoutExpandedGroup`) —
   *  mirrors the existing `onOrbit` convention: only nodes flagged this way
   *  are eligible for the expanded-ring layout/draw pass. */
  onExpandedRing?: boolean;
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
  /** Hex layout only: circumradius of one file's cell, in graph units — the
   *  renderer multiplies by its own px radius to draw matching-size cells. */
  hexUnit?: number;
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
  /** The colours areas, folders and user apps pick from (`themeSwatches`). */
  areaSwatches: string[];
  light: boolean;
}

const clamp = (value: number, min: number, max: number): number => Math.min(max, Math.max(min, value));

/** Reads `#rgb`, `#rrggbb` and `rgb(r, g, b)` into 0…255 channels; null for anything else (named, `hsl()`, …). */
function parseRgb(css: string): [number, number, number] | null {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(css.trim());
  if (hex?.[1]) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((d) => d + d).join("") : hex[1];
    return [0, 2, 4].map((i) => parseInt(digits.slice(i, i + 2), 16)) as [number, number, number];
  }
  const rgb = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(css.trim());
  return rgb ? [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])] : null;
}

/** Hue (0–360), saturation and lightness (0–100) of a colour given as 0…255 channels. */
function toHsl([r, g, b]: [number, number, number]): { h: number; s: number; l: number } {
  const [rn, gn, bn] = [r / 255, g / 255, b / 255];
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return { h: 0, s: 0, l: l * 100 };
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = max === rn ? ((gn - bn) / d) % 6 : max === gn ? (bn - rn) / d + 2 : (rn - gn) / d + 4;
  return { h: (h * 60 + 360) % 360, s: s * 100, l: l * 100 };
}

const SWATCH_COUNT = 12;
/** The hue range (degrees) that reads as green and is calmed on dark grounds. */
const GREEN_FROM = 60;
const GREEN_TO = 170;

/**
 * The colours areas, folders and user apps are drawn in: twelve hues around the whole wheel, so neighbouring areas stay
 * apart in every theme, with the saturation and lightness of the theme's accent — pastel where the theme is soft, deep
 * where it is strong, always readable on its background. (The theme's syntax colours were tried and are too close
 * together in themes such as GitHub's, where nearly all of them are blue.) An accent that cannot be read gives the
 * standard tones of the scheme.
 */
export function themeSwatches(accent: string, light: boolean): string[] {
  const rgb = parseRgb(accent);
  const accentHsl = rgb ? toHsl(rgb) : null;
  const s = accentHsl ? clamp(accentHsl.s, 45, 80) : light ? 55 : 70;
  const l = accentHsl ? (light ? clamp(accentHsl.l * 0.85, 34, 48) : clamp(accentHsl.l, 60, 74)) : light ? 42 : 68;
  // Never exactly the accent's own hue.
  const start = (accentHsl?.h ?? 0) + 15;
  return Array.from({ length: SWATCH_COUNT }, (_, i) => {
    const hue = Math.round((start + (360 / SWATCH_COUNT) * i) % 360);
    // On a dark ground the greens (yellow to cyan) shine far brighter than the other hues at the same lightness.
    const calm = !light && hue >= GREEN_FROM && hue <= GREEN_TO;
    return `hsl(${hue} ${Math.round(calm ? s * 0.68 : s)}% ${Math.round(calm ? l - 8 : l)}%)`;
  });
}

/** Reads the palette from the `--ax-*` tokens currently in effect. */
export function readPalette(): Palette {
  const cs = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback;
  const accent = v("--ax-accent", "#ff7a1a");
  const light = v("--ax-color-scheme", "dark") === "light";
  return {
    text: v("--ax-text", "#f5f5f7"),
    muted: v("--ax-text-muted", "#94949c"),
    accent,
    warning: v("--ax-warning", "#e6b45f"),
    success: v("--ax-success", "#4fd67f"),
    border: v("--ax-border-strong", "#3b3b43"),
    invert: v("--ax-text-invert", "#0b0b0d"),
    surface: v("--ax-surface-1", "#121216"),
    areaSwatches: themeSwatches(accent, light),
    light,
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
  [/\bmail\b|e-?mail/i, "mail"],
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

/** Which `drawGlyph` icon a builtin App-Ring node uses, by its registry
 *  `type`. A hand-drawn vector glyph — deliberately **not** the module's
 *  own `ModuleDefinition.icon` SVG string rasterized into an image: that
 *  was tried first (`graph/appIcons.ts`, since removed) and, twice, failed
 *  to actually render in the real app (a malformed data URL, then a
 *  dimensionless-SVG sizing issue once that was fixed) — plausible WebKit
 *  data-URI/image-loading quirks that were hard to diagnose without a
 *  visual test loop. `drawGlyph`'s existing vector-path glyphs are already
 *  proven reliable everywhere else on this exact canvas (skills, routines,
 *  area folders), so reusing that mechanism trades "the module's literal
 *  icon" for "something that reliably draws at all." `skill`/`routine`/
 *  `mail` are the same glyphs those kinds already use elsewhere in the
 *  graph, for visual consistency; `terminal` (Checkpoint 5c) likewise reuses
 *  an id that already existed in `render.ts`'s `GLYPH_CODEPOINTS` for the
 *  App-Ring group icon picker, rather than being new artwork. Falls back to
 *  `"folder"` (matching `glyphForArea`'s own fallback) for any module not
 *  listed here, so a
 *  future builtin never renders nothing while it waits for a proper glyph. */
export function glyphForModuleType(type: string): string {
  switch (type) {
    case "memory-status":
      return "book";
    case "skills-deck":
      return "skill";
    case "routines-board":
      return "routine";
    case "todo":
      return "check";
    case "calendar":
      return "calendar";
    case "reminders":
      return "list";
    case "mail":
      return "mail";
    case "terminal":
      return "terminal";
    case "view:kanban":
      return "kanban";
    case "view:ide":
      return "code-blocks";
    case "view:editor":
      return "edit-document";
    default:
      return "folder";
  }
}

/**
 * Stable per-area colour from the name. With `swatches` (the theme's own hues, see {@link Palette.areaSwatches}) the
 * name picks one of them, so the cloud follows the theme; without any, a hue from the wheel stands in
 * (saturation/lightness by scheme).
 */
export function areaColor(name: string, light: boolean, swatches: readonly string[] = []): string {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  const swatch = swatches[h % swatches.length];
  if (swatch !== undefined) return swatch;
  const hue = (h % 12) * 30 + 200; // spread across the wheel, offset from the orange accent
  return light ? `hsl(${hue % 360} 55% 42%)` : `hsl(${hue % 360} 70% 68%)`;
}

// Owner feedback (2026-09-13): bumped from 0.06 once the area nodes'
// own circles grew (see the hub's `r` comment above) and started sitting
// closer together at the segment midpoints this gap also drives (see
// `layout.ts`'s area-node placement).
const AREA_GAP = 0.1; // radians between segments
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

/** Stable twinkle-phase offset from a string (a node id/name/path) — every
 *  node kind in `buildModel` derives its `phase` from this, so pulled out
 *  once instead of re-declared per kind. */
export function nodePhase(s: string): number {
  let h = 0;
  for (const ch of s) h = (h * 33 + ch.charCodeAt(0)) >>> 0;
  return (h % 1000) / 1000;
}

export function buildModel(g: WorkspaceGraph, palette: Palette): GraphModel {
  const nodes: GraphNode[] = [];
  const byId = new Map<string, GraphNode>();
  const add = (n: GraphNode) => {
    nodes.push(n);
    byId.set(n.id, n);
  };
  const phase = nodePhase;

  add({
    id: "hub",
    kind: "hub",
    label: g.hub ?? "AGENTS.md",
    area: null,
    path: g.hub ?? undefined,
    bytes: 0,
    x: 0,
    y: 0,
    // Owner feedback (2026-09-13, three rounds): every structural node's
    // circle read a bit small next to its icon-font glyph (`render.ts`'s
    // `drawGlyph` sizes off this same `r`) — hub/skill first (rounds 1-2,
    // since those were called out first), area/routine caught up to match
    // once the mismatch against the still-small outer rings was pointed
    // out (round 3, see their own `r` below). Only `file` (the point-cloud
    // dots, sized by byte count, not by kind) is unaffected.
    r: 18,
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
      r: 14, // see the hub's own `r` comment above
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
    const color = areaColor(a.name, palette.light, palette.areaSwatches);
    const seg = { name: a.name, color, count: a.files, start: angle, end: angle + span };
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
      r: 13, // see the hub's own `r` comment above
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
  // Hub spokes: skills and areas hang off the hub; files off
  // their area node (drawn very faintly).
  for (const n of nodes) {
    if (n.kind === "skill" || n.kind === "area") edges.push({ from: "hub", to: n.id });
    if (n.kind === "file" && n.area) edges.push({ from: `area:${n.area}`, to: n.id });
  }

  return { nodes, edges, areas, byId, totalFiles: g.total_files, truncated: g.truncated };
}

function builtinAppNode(b: BuiltinApp, palette: Palette): GraphNode {
  return {
    id: `app:builtin:${b.type}`,
    kind: "app",
    label: b.title,
    area: null,
    bytes: 0,
    x: 0,
    y: 0,
    r: 9,
    color: palette.accent,
    phase: nodePhase(b.type),
    degree: 0,
    appType: b.type,
    glyph: glyphForModuleType(b.type),
  };
}

function userAppNode(a: UserApp, palette: Palette): GraphNode {
  return {
    id: `app:user:${a.path}`,
    kind: "app",
    label: a.name,
    area: null,
    bytes: 0,
    x: 0,
    y: 0,
    r: 9,
    color: areaColor(a.path, palette.light, palette.areaSwatches),
    phase: nodePhase(a.path),
    degree: 0,
    userApp: true,
    appPath: a.path,
    // Only set when the owner picked one via "Symbol ändern" — omitting it
    // otherwise (rather than a fallback like "folder") is what lets
    // render.ts's drawAppRing tell "no custom icon, draw the monogram"
    // apart from "has one, draw it".
    ...(a.glyph ? { glyph: a.glyph } : {}),
  };
}

/** Builds the App-Ring's node objects from the builtin-module list, the
 *  owner's added Mac apps, and the current App-Ring groups (see
 *  `core/appGroups.ts`) — colour/glyph already set, position left at
 *  `(0, 0)` for `layoutAppRing`/`layoutExpandedGroup` (`./layout`) to fill
 *  in. Independent of `buildModel`/`WorkspaceGraph`: the ring's contents
 *  track the module registry, the `userApps` store and the `appGroups`
 *  store, not a workspace refresh, so `second-brain.svelte` calls this
 *  separately and appends the result to `model.nodes`.
 *
 *  A builtin/user app that's a member of a group gets no solo ring-slot
 *  node of its own — the group's own node stands in for it. Each non-empty
 *  group gets exactly one ring-slot node (`isGroup: true`); additionally,
 *  *only* for `g.id === expandedGroupId` (at most one at a time — see
 *  `second-brain.svelte`), one node per member is emitted too
 *  (`onExpandedRing: true`), for `layoutExpandedGroup` to place on the
 *  secondary ring. A collapsed group therefore has zero member nodes to
 *  accidentally hit-test or draw — see the App-Ring-Gruppierung plan's
 *  Checkpoint 3 rationale for why that's the load-bearing simplification
 *  here, instead of keeping every member node alive and toggling
 *  visibility. A member referencing a since-removed builtin/app is skipped
 *  silently, the same convention `buildModel` already uses for edges whose
 *  endpoint no longer exists. */
export function buildAppNodes(
  builtins: BuiltinApp[],
  userApps: UserApp[],
  groups: AppGroup[],
  expandedGroupId: string | null,
  palette: Palette,
): GraphNode[] {
  const groupedBuiltinTypes = new Set(groups.filter((g) => g.side === "builtin").flatMap((g) => g.members));
  const groupedUserPaths = new Set(groups.filter((g) => g.side === "user").flatMap((g) => g.members));
  const builtinByType = new Map(builtins.map((b) => [b.type, b]));
  const userByPath = new Map(userApps.map((a) => [a.path, a]));

  const builtinNodes = builtins.filter((b) => !groupedBuiltinTypes.has(b.type)).map((b) => builtinAppNode(b, palette));
  const userNodes = userApps.filter((a) => !groupedUserPaths.has(a.path)).map((a) => userAppNode(a, palette));

  const groupNodes: GraphNode[] = [];
  const expandedMemberNodes: GraphNode[] = [];
  for (const g of groups) {
    if (g.members.length === 0) continue; // shouldn't happen (appGroups.ts auto-dissolves at 0), stay defensive
    groupNodes.push({
      id: `app:group:${g.id}`,
      kind: "app",
      label: g.name,
      area: null,
      bytes: 0,
      x: 0,
      y: 0,
      r: 12, // somewhat bigger than a solo app's 9 — matches render.ts's drawAppRing scaling for isGroup nodes
      color: g.side === "user" ? areaColor(g.id, palette.light, palette.areaSwatches) : palette.accent,
      phase: nodePhase(g.id),
      degree: 0,
      userApp: g.side === "user",
      glyph: g.glyph,
      isGroup: true,
      groupId: g.id,
    });

    if (g.id !== expandedGroupId) continue;
    for (const memberId of g.members) {
      let node: GraphNode | null = null;
      if (g.side === "builtin") {
        const b = builtinByType.get(memberId);
        if (b) node = builtinAppNode(b, palette);
      } else {
        const a = userByPath.get(memberId);
        if (a) node = userAppNode(a, palette);
      }
      if (!node) continue;
      expandedMemberNodes.push({ ...node, groupId: g.id, onExpandedRing: true });
    }
  }

  return [...builtinNodes, ...userNodes, ...groupNodes, ...expandedMemberNodes];
}
