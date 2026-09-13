import { beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import type { GraphNode } from "../graph/model";
import {
  addToGroup,
  appGroups,
  createGroup,
  groupFor,
  loadAppGroups,
  menuActionsFor,
  nextGroupName,
  removeFromGroup,
  removeMemberFromAllGroups,
  renameGroup,
  setGroupGlyph,
} from "./appGroups";

beforeEach(() => loadAppGroups([]));

/** A minimal "app" GraphNode for `menuActionsFor` tests — only the fields
 *  that function actually reads vary per test, the rest are filler. */
function appNode(overrides: Partial<GraphNode> = {}): GraphNode {
  return {
    id: "app:x",
    kind: "app",
    label: "X",
    area: null,
    bytes: 0,
    x: 0,
    y: 0,
    r: 9,
    color: "#f70",
    phase: 0,
    degree: 0,
    ...overrides,
  };
}

describe("nextGroupName", () => {
  it("starts at 1", () => {
    expect(nextGroupName([])).toBe("Gruppe 1");
  });

  it("fills gaps instead of always counting up", () => {
    expect(nextGroupName(["Gruppe 1", "Gruppe 3"])).toBe("Gruppe 2");
  });

  it("ignores names that don't match the pattern", () => {
    expect(nextGroupName(["Meine Gruppe", "Gruppe 1"])).toBe("Gruppe 2");
  });
});

describe("createGroup / addToGroup / removeFromGroup", () => {
  it("creates a one-member group with a default name and glyph", () => {
    const g = createGroup("user", "/Applications/Foo.app");
    expect(g).toMatchObject({ side: "user", name: "Gruppe 1", glyph: "group", members: ["/Applications/Foo.app"] });
    expect(get(appGroups)).toEqual([g]);
  });

  it("names successive groups on the same side independently per side", () => {
    createGroup("user", "/Applications/Foo.app");
    const b = createGroup("builtin", "todo");
    expect(b.name).toBe("Gruppe 1"); // builtin side starts fresh, not "Gruppe 2"
  });

  it("addToGroup appends, ignoring an already-present member", () => {
    const g = createGroup("user", "/Applications/Foo.app");
    addToGroup(g.id, "/Applications/Bar.app");
    addToGroup(g.id, "/Applications/Bar.app");
    expect(get(appGroups).find((x) => x.id === g.id)?.members).toEqual([
      "/Applications/Foo.app",
      "/Applications/Bar.app",
    ]);
  });

  it("removeFromGroup dissolves the group once its last member is removed", () => {
    const g = createGroup("user", "/Applications/Foo.app");
    addToGroup(g.id, "/Applications/Bar.app");
    removeFromGroup(g.id, "/Applications/Foo.app");
    expect(get(appGroups)).toHaveLength(1); // still one member left
    removeFromGroup(g.id, "/Applications/Bar.app");
    expect(get(appGroups)).toEqual([]); // 0 members -> group gone
  });
});

describe("removeMemberFromAllGroups", () => {
  it("only strips the member from groups on the matching side", () => {
    const userGroup = createGroup("user", "shared-id");
    const builtinGroup = createGroup("builtin", "shared-id");
    removeMemberFromAllGroups("user", "shared-id");
    expect(get(appGroups).find((g) => g.id === userGroup.id)).toBeUndefined(); // dissolved
    expect(get(appGroups).find((g) => g.id === builtinGroup.id)?.members).toEqual(["shared-id"]); // untouched
  });
});

describe("groupFor", () => {
  it("finds the group a member belongs to, scoped to side", () => {
    const g = createGroup("user", "/Applications/Foo.app");
    expect(groupFor("user", "/Applications/Foo.app")).toEqual(g);
    expect(groupFor("builtin", "/Applications/Foo.app")).toBeUndefined();
  });
});

describe("renameGroup / setGroupGlyph", () => {
  it("renames, trimming whitespace", () => {
    const g = createGroup("user", "/Applications/Foo.app");
    renameGroup(g.id, "  Arbeit  ");
    expect(get(appGroups).find((x) => x.id === g.id)?.name).toBe("Arbeit");
  });

  it("ignores a blank/whitespace-only rename", () => {
    const g = createGroup("user", "/Applications/Foo.app");
    renameGroup(g.id, "   ");
    expect(get(appGroups).find((x) => x.id === g.id)?.name).toBe("Gruppe 1");
  });

  it("changes the glyph", () => {
    const g = createGroup("user", "/Applications/Foo.app");
    setGroupGlyph(g.id, "mail");
    expect(get(appGroups).find((x) => x.id === g.id)?.glyph).toBe("mail");
  });
});

describe("menuActionsFor", () => {
  it("a group node offers only rename + change icon", () => {
    const node = appNode({ isGroup: true, groupId: "g1" });
    expect(menuActionsFor(node, [])).toEqual([{ kind: "rename" }, { kind: "changeIcon" }]);
  });

  it("a member on an expanded ring offers only remove-from-group", () => {
    const node = appNode({ groupId: "g1" });
    expect(menuActionsFor(node, [])).toEqual([{ kind: "removeFromGroup" }]);
  });

  it("an ungrouped builtin offers grouping actions but never remove", () => {
    const node = appNode({ userApp: undefined });
    const actions = menuActionsFor(node, []);
    expect(actions).toEqual([{ kind: "addToNewGroup" }]);
  });

  it("an ungrouped user app offers grouping actions plus change icon and remove", () => {
    const node = appNode({ userApp: true });
    const actions = menuActionsFor(node, []);
    expect(actions).toEqual([{ kind: "addToNewGroup" }, { kind: "changeIcon" }, { kind: "remove" }]);
  });

  it("offers 'add to <existing group>' only for groups on the same side", () => {
    const node = appNode({ userApp: true });
    const groups = [
      { id: "g1", side: "user" as const, name: "Arbeit", glyph: "group", members: ["a"] },
      { id: "g2", side: "builtin" as const, name: "System", glyph: "group", members: ["b"] },
    ];
    expect(menuActionsFor(node, groups)).toEqual([
      { kind: "addToNewGroup" },
      { kind: "addToGroup", groupId: "g1", groupName: "Arbeit" },
      { kind: "changeIcon" },
      { kind: "remove" },
    ]);
  });
});
