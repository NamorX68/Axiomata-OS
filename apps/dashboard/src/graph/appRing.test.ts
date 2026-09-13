import { describe, expect, it } from "vitest";

import type { BuiltinApp, UserApp } from "../core/apps";
import { APP_RING, layoutAppRing } from "./layout";
import { areaColor, buildAppNodes, glyphForModuleType, type GraphNode, type Palette } from "./model";

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

const builtins: BuiltinApp[] = [
  { type: "memory-status", title: "Memory" },
  { type: "todo", title: "ToDo" },
];

const userApps: UserApp[] = [
  { path: "/Applications/Foo.app", name: "Foo" },
  { path: "/Applications/Bar.app", name: "Bar" },
];

function angleOf(n: GraphNode): number {
  return Math.atan2(n.y, n.x);
}

function place(): { builtinNodes: GraphNode[]; userNodes: GraphNode[] } {
  const nodes = buildAppNodes(builtins, userApps, palette);
  const builtinNodes = nodes.filter((n) => !n.userApp);
  const userNodes = nodes.filter((n) => n.userApp);
  layoutAppRing(builtinNodes, userNodes);
  return { builtinNodes, userNodes };
}

describe("buildAppNodes", () => {
  it("marks builtin nodes as not userApp, user-app nodes as userApp", () => {
    const nodes = buildAppNodes(builtins, userApps, palette);
    const builtinNodes = nodes.filter((n) => n.id.startsWith("app:builtin:"));
    const userNodes = nodes.filter((n) => n.id.startsWith("app:user:"));
    expect(builtinNodes).toHaveLength(2);
    expect(userNodes).toHaveLength(2);
    expect(builtinNodes.every((n) => !n.userApp)).toBe(true);
    expect(userNodes.every((n) => n.userApp === true)).toBe(true);
  });

  it("carries the registry type/glyph for builtins and the path for user apps", () => {
    const nodes = buildAppNodes(builtins, userApps, palette);
    const memory = nodes.find((n) => n.id === "app:builtin:memory-status");
    expect(memory).toMatchObject({
      kind: "app",
      appType: "memory-status",
      glyph: glyphForModuleType("memory-status"),
      color: palette.accent,
    });
    const foo = nodes.find((n) => n.id === "app:user:/Applications/Foo.app");
    expect(foo).toMatchObject({ kind: "app", appPath: "/Applications/Foo.app", label: "Foo" });
  });

  it("gives user apps exactly areaColor(path, light) — not just some path-stable value", () => {
    const a = buildAppNodes([], [{ path: "/Applications/Foo.app", name: "Foo" }], palette);
    const b = buildAppNodes([], [{ path: "/Applications/Foo.app", name: "Renamed" }], palette);
    // Stable regardless of display name (the plan's actual requirement)...
    expect(a[0].color).toBe(b[0].color);
    // ...and specifically the same formula/value area nodes use, per
    // `render.ts`'s own doc comment — not merely "some" stable colour a
    // different, undocumented formula could also have produced.
    expect(a[0].color).toBe(areaColor("/Applications/Foo.app", palette.light));
  });
});

describe("layoutAppRing", () => {
  it("keeps the \"+\" at -π/2 free — no node sits exactly on it", () => {
    const { builtinNodes, userNodes } = place();
    for (const n of [...builtinNodes, ...userNodes]) {
      expect(angleOf(n)).not.toBeCloseTo(-Math.PI / 2, 5);
    }
  });

  it("places builtins in (-π, -π/2), user apps in (-π/2, 0)", () => {
    const { builtinNodes, userNodes } = place();
    for (const n of builtinNodes) {
      const a = angleOf(n);
      expect(a).toBeLessThan(-Math.PI / 2);
      expect(a).toBeGreaterThan(-Math.PI);
    }
    for (const n of userNodes) {
      const a = angleOf(n);
      expect(a).toBeGreaterThan(-Math.PI / 2);
      expect(a).toBeLessThan(0);
    }
  });

  it("places the first entry on each side closest to the \"+\"", () => {
    const { builtinNodes, userNodes } = place();
    // Builtins count outward (more negative angle) as index grows.
    expect(angleOf(builtinNodes[0])).toBeGreaterThan(angleOf(builtinNodes[1]));
    // User apps count outward (less negative / more positive angle) as index grows.
    expect(angleOf(userNodes[0])).toBeLessThan(angleOf(userNodes[1]));
  });

  it("is deterministic — the same input always yields the same positions", () => {
    const first = place();
    const second = place();
    expect(first.builtinNodes.map((n) => [n.x, n.y])).toEqual(second.builtinNodes.map((n) => [n.x, n.y]));
    expect(first.userNodes.map((n) => [n.x, n.y])).toEqual(second.userNodes.map((n) => [n.x, n.y]));
  });

  it("places every node at the APP_RING radius", () => {
    const { builtinNodes, userNodes } = place();
    for (const n of [...builtinNodes, ...userNodes]) {
      expect(Math.hypot(n.x, n.y)).toBeCloseTo(APP_RING, 10);
    }
  });
});
