import { describe, expect, it } from "vitest";

import type { WorkspaceGraph } from "../core/backend";
import { applyLayout, RING } from "./layout";
import { buildModel, glyphForArea, neighbours, regroup, searchNodes, type Palette } from "./model";

const palette: Palette = {
  text: "#fff",
  muted: "#888",
  accent: "#f70",
  warning: "#fc0",
  success: "#0f0",
  border: "#444",
  invert: "#000",
  surface: "#111",
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
    skills: [{ name: "a", description: "", backend: "claude-code", model: null, effort: null }],
    routines: [],
    total_files: 41,
    truncated: false,
    generated_at: "",
  };
}

describe("buildModel", () => {
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

  it("circle puts every area file on one radius", () => {
    const m = buildModel(fixture(), palette);
    applyLayout(m, "circle");
    const radii = new Set(
      m.nodes.filter((n) => n.kind === "file" && n.area).map((n) => Math.hypot(n.x, n.y).toFixed(6)),
    );
    expect(radii.size).toBe(1);
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
