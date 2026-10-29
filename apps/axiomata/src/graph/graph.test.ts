import { describe, expect, it } from "vitest";

import type { WorkspaceGraph } from "../core/backend";
import { BUNDLE_ROOT, bundleControls, dirKey } from "./bundle";
import {
  applyLayout,
  approachAngle,
  BUNDLE,
  layoutOrbit,
  MAP_EDGE,
  MAP_SKILLS,
  orbitRestAngle,
  RING,
  shortestTurn,
} from "./layout";
import { buildModel, glyphForArea, neighbours, regroup, searchNodes, type Palette } from "./model";
import { cellKey, NEIGHBOURS } from "./territory";

const palette: Palette = {
  text: "#fff",
  muted: "#888",
  accent: "#f70",
  warning: "#fc0",
  success: "#0f0",
  border: "#444",
  invert: "#000",
  surface: "#111",
  areaSwatches: ["#a00", "#0a0", "#00a"],
  glow: "#f80",
  light: false,
};

function fixture(): WorkspaceGraph {
  const files = [];
  for (const [area, n] of [
    ["Dev", 30],
    ["Work", 10],
  ] as const) {
    for (let i = 0; i < n; i++) {
      files.push({ path: `${area}/sub${i % 2}/n${i}.md`, area, title: `N${i} ${area}`, bytes: 100 * (i + 1), modified: null, is_markdown: true });
    }
  }
  files.push({ path: "loose.md", area: null, title: "Loose", bytes: 10, modified: null, is_markdown: true });
  return {
    workspace_root: "/w",
    hub: "CLAUDE.md",
    areas: [
      { name: "Dev", files: 30 },
      { name: "Work", files: 10 },
    ],
    files,
    links: [
      { from: "Dev/sub0/n0.md", to: "Work/sub1/n1.md" },
      { from: "Dev/sub0/n0.md", to: "missing.md" },
    ],
    skills: [{ name: "a", description: "", backend: "opencode", model: null, effort: null }],
    routines: [],
    total_files: 41,
    truncated: false,
    generated_at: "",
  };
}

describe("buildModel", () => {
  it("draws no routines: they live on the Routines board, not in the graph", () => {
    const graph = fixture();
    graph.routines = [
      { id: 1, name: "Nightly", cron_expr: "0 0 3 * * *", target: { type: "skill", value: "a" }, backend: null, enabled: true, next_fire_at: null, last_fired_at: null },
    ];
    const m = buildModel(graph, palette);
    expect(m.nodes.map((n) => n.id).filter((id) => id.startsWith("routine"))).toEqual([]);
    expect(m.edges).toHaveLength(1 + 3 + 40);
  });

  it("creates hub, skill and file nodes with segments proportional to counts", () => {
    const m = buildModel(fixture(), palette);
    expect(m.nodes.filter((n) => n.kind === "hub")).toHaveLength(1);
    expect(m.nodes.filter((n) => n.kind === "skill")).toHaveLength(1);
    expect(m.nodes.filter((n) => n.kind === "file")).toHaveLength(41);
    expect(m.nodes.filter((n) => n.kind === "area").map((n) => n.label)).toEqual(["Dev", "Work"]);
    const [dev, work] = m.areas;
    expect(dev.end - dev.start).toBeCloseTo((work.end - work.start) * 3, 5);
    // edges: one resolvable link + hub spokes (skill, 2 areas) + 40 area→file
    expect(m.edges).toHaveLength(1 + 3 + 40);
    expect(m.byId.get("file:Dev/sub0/n0.md")?.degree).toBe(1);
    expect(m.byId.get("skill:a")?.color).toBe(palette.accent);
  });
});

describe("layouts", () => {
  it("bundle puts every note on one ring and the skills on a ring further out, nothing in the middle", () => {
    const m = buildModel(fixture(), palette);
    applyLayout(m, "bundle");
    for (const n of m.nodes.filter((x) => x.kind === "file")) expect(Math.hypot(n.x, n.y)).toBeCloseTo(BUNDLE.notes, 9);
    expect(Math.hypot(m.byId.get("skill:a")!.x, m.byId.get("skill:a")!.y)).toBeCloseTo(BUNDLE.skills, 9);
    // The area's icon sits outside the ring, at the middle of its arc.
    const dev = m.bundle!.arcs.find((a) => a.area === "Dev")!;
    const icon = m.byId.get("area:Dev")!;
    expect(Math.hypot(icon.x, icon.y)).toBeCloseTo(BUNDLE.areas, 9);
    expect(Math.atan2(icon.y, icon.x)).toBeCloseTo((dev.start + dev.end) / 2, 9);
    expect(BUNDLE.areas).toBeGreaterThan(BUNDLE.notes);
    expect(BUNDLE.skills).toBeGreaterThan(BUNDLE.names);
  });

  it("bundle gives each area one arc, sized by its notes, with gaps between, around the whole ring", () => {
    const m = buildModel(fixture(), palette);
    applyLayout(m, "bundle");
    const arcs = m.bundle!.arcs;
    expect(arcs.map((a) => a.area)).toEqual(["Dev", "Work", null]);
    expect(arcs.map((a) => a.count)).toEqual([30, 10, 1]);
    const span = (a: { start: number; end: number }) => a.end - a.start;
    expect(span(arcs[0]!)).toBeCloseTo(span(arcs[1]!) * 3, 9);
    for (let i = 1; i < arcs.length; i++) expect(arcs[i]!.start).toBeGreaterThan(arcs[i - 1]!.end);
    const gaps = arcs.reduce((sum, a, i) => sum + ((arcs[(i + 1) % arcs.length]!.start - a.end + 2 * Math.PI) % (2 * Math.PI)), 0);
    expect(gaps + arcs.reduce((sum, a) => sum + span(a), 0)).toBeCloseTo(2 * Math.PI, 9);
    // Every note of an area lies inside its arc.
    for (const arc of arcs) {
      for (const n of m.nodes.filter((x) => x.kind === "file" && x.area === arc.area)) {
        let a = Math.atan2(n.y, n.x);
        while (a < arc.start) a += 2 * Math.PI;
        expect(a).toBeLessThanOrEqual(arc.end);
      }
    }
  });

  it("bundle lays an arc's newest note first and the older ones after it, clockwise", () => {
    const graph = fixture();
    graph.files.forEach((f, i) => (f.created = new Date(Date.UTC(2025, 0, 1) + ((i * 7) % 40) * 86_400_000).toISOString()));
    const m = buildModel(graph, palette);
    applyLayout(m, "bundle");
    const dev = m.bundle!.arcs.find((a) => a.area === "Dev")!;
    // Clockwise on screen is the growing angle (y points down), measured from the arc's start.
    const along = (n: { x: number; y: number }) => {
      let a = Math.atan2(n.y, n.x);
      while (a < dev.start) a += 2 * Math.PI;
      return a;
    };
    const clockwise = m.nodes
      .filter((n) => n.kind === "file" && n.area === "Dev")
      .sort((a, b) => along(a) - along(b))
      .map((n) => Date.parse(n.created!));
    expect(clockwise).toEqual([...clockwise].sort((a, b) => b - a));
  });

  it("bundle grouped by folders splits the ring finer, siblings side by side", () => {
    const m = buildModel(regroup(fixture(), "folders"), palette);
    applyLayout(m, "bundle");
    expect(m.bundle!.arcs.map((a) => a.area)).toEqual(["Dev/sub0", "Dev/sub1", "Work/sub0", "Work/sub1", null]);
    const parent = m.bundle!.parent;
    expect(parent.get("file:Dev/sub0/n0.md")).toBe(dirKey("Dev/sub0"));
    expect(parent.get(dirKey("Dev/sub0"))).toBe(dirKey("Dev"));
    expect(parent.get(dirKey("Dev"))).toBe(BUNDLE_ROOT);
    expect(parent.get("file:loose.md")).toBe(BUNDLE_ROOT);
    // A folder's point lies between the ring and the centre, its parent's further in.
    const r = (key: string) => Math.hypot(m.bundle!.points.get(key)!.x, m.bundle!.points.get(key)!.y);
    expect(r(dirKey("Dev/sub0"))).toBeLessThan(BUNDLE.notes);
    expect(r(dirKey("Dev"))).toBeLessThan(r(dirKey("Dev/sub0")));
    expect(r(BUNDLE_ROOT)).toBe(0);
  });

  it("bundle runs a link's control points up the tree to the common ancestor and down again", () => {
    const at = (m: ReturnType<typeof buildModel>, key: string) => m.bundle!.points.get(key)!;
    const byArea = buildModel(fixture(), palette);
    applyLayout(byArea, "bundle");
    const a = "file:Dev/sub0/n0.md";
    const b = "file:Work/sub1/n1.md";
    expect(bundleControls(byArea.bundle!, a, b)).toEqual(
      [a, dirKey("Dev"), BUNDLE_ROOT, dirKey("Work"), b].map((k) => at(byArea, k)),
    );
    const byFolder = buildModel(regroup(fixture(), "folders"), palette);
    applyLayout(byFolder, "bundle");
    expect(bundleControls(byFolder.bundle!, a, b)).toEqual(
      [a, dirKey("Dev/sub0"), dirKey("Dev"), BUNDLE_ROOT, dirKey("Work"), dirKey("Work/sub1"), b].map((k) =>
        at(byFolder, k),
      ),
    );
    // Two notes of one folder meet in that folder; a note unknown to the layout has no curve.
    const c = "file:Dev/sub0/n2.md";
    expect(bundleControls(byFolder.bundle!, a, c)).toEqual([a, dirKey("Dev/sub0"), c].map((k) => at(byFolder, k)));
    expect(bundleControls(byFolder.bundle!, a, "file:missing.md")).toEqual([]);
  });

  it("orbit puts the hub at the centre, gives every file a 3-D cloud point, and marks the rim", () => {
    const m = buildModel(fixture(), palette);
    layoutOrbit(m);
    const hub = m.byId.get("hub")!;
    expect([hub.x, hub.y]).toEqual([0, 0]);
    const skill = m.byId.get("skill:a")!;
    expect(skill.onOrbit).toBe(true);
    const files = m.nodes.filter((n) => n.kind === "file");
    expect(files.every((n) => n.p3 && n.p3.length === 3)).toBe(true);
    // Areas are parked, never shown, in orbit mode.
    expect(m.byId.get("area:Dev")!.onOrbit).toBeFalsy();
  });

  it("orbit rest angle turns the newest note on the ring to 12 o'clock, older ones follow clockwise", () => {
    const graph = fixture();
    graph.files = graph.files.slice(0, 5).map((f, i) => ({ ...f, modified: `2026-10-0${i + 1}T10:00:00Z` }));
    const m = buildModel(graph, palette);
    layoutOrbit(m);
    const rest = orbitRestAngle(m);
    const ring = m.nodes.filter((n) => n.onOrbit && n.kind === "file");
    const shown = (n: { x: number; y: number }): number =>
      (((Math.atan2(n.y, n.x) + rest + Math.PI / 2) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
    const newest = ring.reduce((a, b) => (Date.parse(a.modified!) > Date.parse(b.modified!) ? a : b));
    const topAngle = shown(newest);
    expect(Math.min(topAngle, 2 * Math.PI - topAngle)).toBeCloseTo(0, 6);
    // Clockwise from the top, the modified times fall.
    const clockwise = [...ring].sort((a, b) => shown(a) - shown(b)).map((n) => Date.parse(n.modified!));
    expect(clockwise).toEqual([...clockwise].sort((a, b) => b - a));
    // The skill keeps its slot (before the notes), only the ring as a whole is turned.
    expect(m.byId.get("skill:a")!.onOrbit).toBe(true);
  });

  it("orbit rest angle is 0 when no note is on the ring", () => {
    const m = buildModel(fixture(), palette);
    layoutOrbit(m);
    expect(orbitRestAngle(m)).toBe(0);
  });

  it("hex gives every file its own cell, tiled in its area's wedge, hub and skills in the middle", () => {
    const m = buildModel(fixture(), palette);
    applyLayout(m, "hex");
    const hub = m.byId.get("hub")!;
    expect([hub.x, hub.y]).toEqual([0, 0]);
    const skill = m.byId.get("skill:a")!;
    expect(Math.hypot(skill.x, skill.y)).toBeCloseTo(RING.skills, 5);
    expect(m.hexUnit).toBeGreaterThan(0);
    const files = m.nodes.filter((n) => n.kind === "file");
    // Every file lands on a distinct cell — no two share a position.
    const seen = new Set(files.map((n) => `${n.x.toFixed(6)},${n.y.toFixed(6)}`));
    expect(seen.size).toBe(files.length);
    // Neighbouring cells in the same axial hex grid sit exactly `hexUnit * √3` apart or more — never closer, which
    // would mean two notes overlapping.
    const minGap = Math.sqrt(3) * m.hexUnit! - 1e-6;
    for (let i = 0; i < files.length; i++) {
      for (let j = i + 1; j < files.length; j++) {
        const d = Math.hypot(files[i]!.x - files[j]!.x, files[i]!.y - files[j]!.y);
        expect(d).toBeGreaterThanOrEqual(minGap);
      }
    }
    // The outermost cell lands on the band's outer edge.
    expect(Math.max(...files.map((n) => Math.hypot(n.x, n.y)))).toBeCloseTo(RING.filesOuter, 6);
  });

  it("hex keeps each area's cells inside its own wedge, so an area's colour lies together", () => {
    const m = buildModel(fixture(), palette);
    applyLayout(m, "hex");
    const twoPi = Math.PI * 2;
    for (const seg of m.areas) {
      for (const n of m.nodes.filter((x) => x.kind === "file" && x.area === seg.name)) {
        let a = Math.atan2(n.y, n.x);
        while (a < seg.start) a += twoPi;
        while (a >= seg.start + twoPi) a -= twoPi;
        expect(a).toBeGreaterThanOrEqual(seg.start);
        expect(a).toBeLessThan(seg.end);
      }
    }
  });

  it("hex lays an area's newest notes nearest the middle", () => {
    const graph = fixture();
    graph.files = graph.files.map((f) => ({ ...f, created: "2025-01-01T00:00:00Z", modified: null }));
    // Dev's notes born one day apart, the last one first in the list.
    graph.files.forEach((f, i) => {
      if (f.area === "Dev") f.created = new Date(Date.UTC(2025, 0, 30 - i)).toISOString();
    });
    const m = buildModel(graph, palette);
    applyLayout(m, "hex");
    const dev = m.nodes
      .filter((n) => n.kind === "file" && n.area === "Dev")
      .sort((a, b) => Date.parse(a.created!) - Date.parse(b.created!));
    const radius = (n: { x: number; y: number }) => Math.hypot(n.x, n.y);
    expect(radius(dev[dev.length - 1]!)).toBeLessThan(radius(dev[0]!));
  });

  it("map gives every file its own field inside the lens, skills ringing it, areas at their country's middle", () => {
    const m = buildModel(fixture(), palette);
    applyLayout(m, "map");
    const skill = m.byId.get("skill:a")!;
    expect(Math.hypot(skill.x, skill.y)).toBeCloseTo(MAP_SKILLS, 5);
    expect(m.hexUnit).toBeGreaterThan(0);
    expect(m.territory?.size).toBe(41);
    const work = m.nodes.filter((n) => n.area === "Work" && n.kind === "file");
    const workArea = m.byId.get("area:Work")!;
    expect(workArea.x).toBeCloseTo(work.reduce((s, n) => s + n.x, 0) / work.length, 6);
    // Every field, corner included, stays inside the map's edge; its neighbours are the notes of the fields around it.
    for (const n of m.nodes.filter((n) => n.kind === "file")) {
      expect(Math.hypot(n.x, n.y) + m.hexUnit!).toBeLessThanOrEqual(MAP_EDGE + 1e-6);
      NEIGHBOURS.forEach(({ dq, dr }, i) => {
        expect(n.around?.[i]).toBe(m.territory!.get(cellKey(n.cell!.q + dq, n.cell!.r + dr)) ?? null);
      });
    }
    const files = m.nodes.filter((n) => n.kind === "file");
    // Every file lands on a distinct cell — no two share a position.
    const seen = new Set(files.map((n) => `${n.x.toFixed(6)},${n.y.toFixed(6)}`));
    expect(seen.size).toBe(files.length);
    // Neighbouring cells in the same axial hex grid sit exactly `hexUnit *
    // √3` apart (row height) or a multiple of it — never closer, which
    // would mean two notes overlapping.
    const minGap = Math.sqrt(3) * m.hexUnit! - 1e-6;
    for (let i = 0; i < files.length; i++) {
      for (let j = i + 1; j < files.length; j++) {
        const d = Math.hypot(files[i].x - files[j].x, files[i].y - files[j].y);
        expect(d).toBeGreaterThanOrEqual(minGap);
      }
    }
  });
});

describe("ring settling into its rest angle", () => {
  const TAU = 2 * Math.PI;

  it("takes the shortest way round, across zero", () => {
    expect(shortestTurn(0.1, TAU - 0.1)).toBeCloseTo(-0.2, 9);
    expect(shortestTurn(TAU - 0.1, 0.1)).toBeCloseTo(0.2, 9);
    expect(shortestTurn(0, Math.PI)).toBeCloseTo(Math.PI, 9);
  });

  it("turns backwards over 0 from 0.1 to 2π−0.1 instead of almost a full round", () => {
    const next = approachAngle(0.1, TAU - 0.1, 0.1, 5);
    expect(next).toBeLessThan(0.1);
    expect(next).toBeGreaterThan(-0.1);
  });

  it("settles on the target exactly and stays there", () => {
    let angle = 3;
    for (let i = 0; i < 300; i++) angle = approachAngle(angle, 0.5, 1 / 60, 5);
    expect(angle).toBe(0.5);
    expect(approachAngle(angle, 0.5, 1 / 60, 5)).toBe(0.5);
  });
});

describe("regroup / search / neighbours", () => {
  it("folders view re-keys areas to parent folders", () => {
    const g = regroup(fixture(), "folders");
    expect(g.areas.map((a) => a.name)).toEqual(["Dev/sub0", "Dev/sub1", "Work/sub0", "Work/sub1"]);
    expect(g.files.find((f) => f.path === "loose.md")?.area).toBeNull();
    const same = fixture();
    expect(regroup(same, "areas")).toBe(same);
  });

  it("search matches every word across label, path and area", () => {
    const m = buildModel(fixture(), palette);
    expect(searchNodes(m, "n1 work").size).toBeGreaterThanOrEqual(1);
    expect(searchNodes(m, "n1 work").has("file:Work/sub1/n1.md")).toBe(true);
    expect(searchNodes(m, "zzz").size).toBe(0);
    expect(searchNodes(m, "  ").size).toBe(0);
  });

  it("neighbours report direction", () => {
    const m = buildModel(fixture(), palette);
    const out = neighbours(m, "file:Dev/sub0/n0.md");
    expect(out).toEqual([
      { node: m.byId.get("file:Work/sub1/n1.md"), out: true },
      { node: m.byId.get("area:Dev"), out: false },
    ]);
    const back = neighbours(m, "file:Work/sub1/n1.md");
    expect(back[0].out).toBe(false);
  });
});

describe("glyphForArea", () => {
  it("matches the owner's real top-level areas", () => {
    expect(glyphForArea("Arbeit")).toBe("briefcase");
    expect(glyphForArea("Entwicklung")).toBe("code");
    expect(glyphForArea("Fotografie")).toBe("camera");
    expect(glyphForArea("Gesellschaft")).toBe("people");
    expect(glyphForArea("KI")).toBe("chip");
    expect(glyphForArea("Persönlich")).toBe("user");
    expect(glyphForArea("System und Werkzeuge")).toBe("wrench");
    expect(glyphForArea("Learning")).toBe("book");
    expect(glyphForArea("Mail")).toBe("mail");
    expect(glyphForArea("Inbox")).toBe("tray");
  });

  it("prefers the more specific course-content match for nested Learning folders", () => {
    // Folders view: these are code-lesson folders, not just "something to
    // read", so the dev/code icon wins over the generic Learning book.
    expect(glyphForArea("Learning/Rust/lessons")).toBe("code");
    expect(glyphForArea("Learning/BlockOS")).toBe("code");
  });

  it("is case-insensitive and falls back to a plain folder for anything unmatched", () => {
    expect(glyphForArea("arbeit")).toBe("briefcase");
    expect(glyphForArea("Random Project")).toBe("folder");
    expect(glyphForArea("")).toBe("folder");
  });
});
