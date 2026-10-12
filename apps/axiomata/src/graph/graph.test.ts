import { describe, expect, it } from "vitest";

import type { WorkspaceGraph } from "../core/backend";
import { applyLayout, approachAngle, layoutOrbit, MAP_EDGE, MAP_SKILLS, orbitRestAngle, RING, shortestTurn } from "./layout";
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
  it("rings keep files inside the band and skills / hub where expected", () => {
    const m = buildModel(fixture(), palette);
    applyLayout(m, "rings");
    const hub = m.byId.get("hub")!;
    expect([hub.x, hub.y]).toEqual([0, 0]);
    const skill = m.byId.get("skill:a")!;
    expect(Math.hypot(skill.x, skill.y)).toBeCloseTo(RING.skills, 5);
    const area = m.byId.get("area:Dev")!;
    expect(Math.hypot(area.x, area.y)).toBeCloseTo(RING.areas, 5);
    for (const n of m.nodes.filter((n) => n.kind === "file" && n.area)) {
      const r = Math.hypot(n.x, n.y);
      expect(r).toBeGreaterThanOrEqual(RING.filesInner - 1e-9);
      expect(r).toBeLessThanOrEqual(RING.filesOuter + 1e-9);
    }
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
