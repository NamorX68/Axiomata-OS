import { describe, expect, it } from "vitest";

import type { AppGroup } from "../core/appGroups";
import type { BuiltinApp, UserApp } from "../core/apps";
import { APP_RING, EXPANDED_GROUP_RING, layoutAppRing, layoutExpandedGroup } from "./layout";
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

function place(groups: AppGroup[] = [], expandedGroupId: string | null = null) {
  const nodes = buildAppNodes(builtins, userApps, groups, expandedGroupId, palette);
  const ringNodes = nodes.filter((n) => !n.onExpandedRing);
  const builtinNodes = ringNodes.filter((n) => !n.userApp);
  const userNodes = ringNodes.filter((n) => n.userApp);
  layoutAppRing(builtinNodes, userNodes);
  return { nodes, builtinNodes, userNodes };
}

describe("buildAppNodes", () => {
  it("marks builtin nodes as not userApp, user-app nodes as userApp", () => {
    const nodes = buildAppNodes(builtins, userApps, [], null, palette);
    const builtinNodes = nodes.filter((n) => n.id.startsWith("app:builtin:"));
    const userNodes = nodes.filter((n) => n.id.startsWith("app:user:"));
    expect(builtinNodes).toHaveLength(2);
    expect(userNodes).toHaveLength(2);
    expect(builtinNodes.every((n) => !n.userApp)).toBe(true);
    expect(userNodes.every((n) => n.userApp === true)).toBe(true);
  });

  it("carries the registry type/glyph for builtins and the path for user apps", () => {
    const nodes = buildAppNodes(builtins, userApps, [], null, palette);
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
    const a = buildAppNodes([], [{ path: "/Applications/Foo.app", name: "Foo" }], [], null, palette);
    const b = buildAppNodes([], [{ path: "/Applications/Foo.app", name: "Renamed" }], [], null, palette);
    // Stable regardless of display name (the plan's actual requirement)...
    expect(a[0].color).toBe(b[0].color);
    // ...and specifically the same formula/value area nodes use, per
    // `render.ts`'s own doc comment — not merely "some" stable colour a
    // different, undocumented formula could also have produced.
    expect(a[0].color).toBe(areaColor("/Applications/Foo.app", palette.light));
  });

  it("carries a user app's own glyph override, omits it when unset", () => {
    const withGlyph = buildAppNodes(
      [],
      [{ path: "/Applications/Foo.app", name: "Foo", glyph: "wrench" }],
      [],
      null,
      palette,
    );
    expect(withGlyph[0].glyph).toBe("wrench");
    const withoutGlyph = buildAppNodes([], [{ path: "/Applications/Foo.app", name: "Foo" }], [], null, palette);
    expect(withoutGlyph[0].glyph).toBeUndefined();
  });

  describe("grouping", () => {
    const userGroup: AppGroup = {
      id: "g1",
      side: "user",
      name: "Arbeit",
      glyph: "briefcase",
      members: ["/Applications/Foo.app", "/Applications/Bar.app"],
    };
    const builtinGroup: AppGroup = {
      id: "g2",
      side: "builtin",
      name: "Tools",
      glyph: "wrench",
      members: ["todo"],
    };

    it("excludes grouped members from the solo ring nodes and adds one node per non-empty group", () => {
      const nodes = buildAppNodes(builtins, userApps, [userGroup, builtinGroup], null, palette);
      expect(nodes.find((n) => n.id === "app:user:/Applications/Foo.app")).toBeUndefined();
      expect(nodes.find((n) => n.id === "app:user:/Applications/Bar.app")).toBeUndefined();
      expect(nodes.find((n) => n.id === "app:builtin:todo")).toBeUndefined();
      expect(nodes.find((n) => n.id === "app:builtin:memory-status")).toBeDefined(); // ungrouped, untouched
      expect(nodes.find((n) => n.id === "app:group:g1")).toMatchObject({
        isGroup: true,
        groupId: "g1",
        label: "Arbeit",
        glyph: "briefcase",
        userApp: true,
      });
      expect(nodes.find((n) => n.id === "app:group:g2")).toMatchObject({
        isGroup: true,
        groupId: "g2",
        label: "Tools",
        glyph: "wrench",
        userApp: false,
      });
    });

    it("emits no member nodes for a collapsed group", () => {
      const nodes = buildAppNodes(builtins, userApps, [userGroup], null, palette);
      expect(nodes.filter((n) => n.onExpandedRing)).toEqual([]);
    });

    it("emits one member node per member, only for the expanded group", () => {
      const nodes = buildAppNodes(builtins, userApps, [userGroup, builtinGroup], "g1", palette);
      const expanded = nodes.filter((n) => n.onExpandedRing);
      expect(expanded.map((n) => n.id).sort()).toEqual(
        ["app:user:/Applications/Foo.app", "app:user:/Applications/Bar.app"].sort(),
      );
      expect(expanded.every((n) => n.groupId === "g1")).toBe(true);
      expect(expanded.every((n) => !n.isGroup)).toBe(true);
    });

    it("silently skips a member id that no longer resolves to a real app", () => {
      const ghostGroup: AppGroup = { id: "g3", side: "user", name: "Ghost", glyph: "group", members: ["/gone.app"] };
      const nodes = buildAppNodes(builtins, userApps, [ghostGroup], "g3", palette);
      expect(nodes.filter((n) => n.onExpandedRing)).toEqual([]);
      expect(nodes.find((n) => n.id === "app:group:g3")).toBeDefined(); // the group node itself still exists
    });
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

describe("layoutExpandedGroup", () => {
  function member(id: string): GraphNode {
    return {
      id,
      kind: "app",
      label: id,
      area: null,
      bytes: 0,
      x: 0,
      y: 0,
      r: 9,
      color: "#fff",
      phase: 0,
      degree: 0,
    };
  }

  it("places a single member exactly at the anchor angle", () => {
    const m = member("a");
    layoutExpandedGroup([m], -Math.PI / 4);
    expect(Math.atan2(m.y, m.x)).toBeCloseTo(-Math.PI / 4, 10);
  });

  it("places every member at the EXPANDED_GROUP_RING radius", () => {
    const members = ["a", "b", "c"].map(member);
    layoutExpandedGroup(members, 0);
    for (const m of members) {
      expect(Math.hypot(m.x, m.y)).toBeCloseTo(EXPANDED_GROUP_RING, 10);
    }
  });

  it("spreads multiple members symmetrically around the anchor angle", () => {
    const members = ["a", "b", "c"].map(member);
    const anchor = Math.PI / 6;
    layoutExpandedGroup(members, anchor);
    const angles = members.map((m) => Math.atan2(m.y, m.x));
    expect(angles[0]).toBeLessThan(anchor);
    expect(angles[1]).toBeCloseTo(anchor, 10); // middle of 3 sits exactly on the anchor
    expect(angles[2]).toBeGreaterThan(anchor);
  });

  it("is deterministic", () => {
    const a = ["a", "b"].map(member);
    const b = ["a", "b"].map(member);
    layoutExpandedGroup(a, 1);
    layoutExpandedGroup(b, 1);
    expect(a.map((m) => [m.x, m.y])).toEqual(b.map((m) => [m.x, m.y]));
  });
});
