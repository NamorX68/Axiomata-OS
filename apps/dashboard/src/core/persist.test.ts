import { describe, expect, it } from "vitest";

import { parseState, sanitizeAppGroups, sanitizeInstances, sanitizeUserApps } from "./persist";

const good = { id: "a", type: "dummy", x: 1, y: 2, w: 100, h: 50 };

describe("sanitizeInstances", () => {
  it("keeps well-formed rows and fills defaults", () => {
    const [inst] = sanitizeInstances([good]);
    expect(inst).toEqual({ ...good, z: 0, flipped: false, config: {} });
  });

  it("drops rows with missing or non-numeric geometry, bad ids, duplicates", () => {
    const rows = [
      good,
      { ...good, id: "a" }, // duplicate id
      { ...good, id: "b", x: "nope" },
      { ...good, id: "", type: "dummy" },
      { ...good, id: "c", type: 42 },
      { ...good, id: "d", w: Number.NaN },
      "not an object",
      null,
    ];
    expect(sanitizeInstances(rows).map((i) => i.id)).toEqual(["a"]);
  });

  it("clamps negative positions and non-positive sizes, coerces config", () => {
    const [inst] = sanitizeInstances([
      { ...good, x: -5, y: -1, w: 0, h: -3, z: 7, flipped: "yes", config: [1] },
    ]);
    expect(inst).toMatchObject({ x: 0, y: 0, w: 1, h: 1, z: 7, flipped: false, config: {} });
  });

  it("returns an empty list for anything that is not an array", () => {
    expect(sanitizeInstances(undefined)).toEqual([]);
    expect(sanitizeInstances({})).toEqual([]);
  });
});

describe("sanitizeUserApps", () => {
  const app = { path: "/Applications/Foo.app", name: "Foo" };

  it("keeps well-formed rows", () => {
    expect(sanitizeUserApps([app])).toEqual([app]);
  });

  it("drops rows with a missing/empty path or name, and duplicate paths", () => {
    const rows = [
      app,
      { ...app, path: "" },
      { ...app, name: "" },
      { path: "/Applications/Bar.app" }, // missing name
      { ...app, name: "Foo (dup path)" }, // same path as `app`, dropped
      "not an object",
      null,
    ];
    expect(sanitizeUserApps(rows)).toEqual([app]);
  });

  it("returns an empty list for anything that is not an array", () => {
    expect(sanitizeUserApps(undefined)).toEqual([]);
    expect(sanitizeUserApps({})).toEqual([]);
  });

  it("carries a well-formed glyph override, drops a malformed one", () => {
    expect(sanitizeUserApps([{ ...app, glyph: "wrench" }])).toEqual([{ ...app, glyph: "wrench" }]);
    expect(sanitizeUserApps([{ ...app, glyph: 5 }])).toEqual([app]);
  });
});

describe("sanitizeAppGroups", () => {
  const group = { id: "g1", side: "user" as const, name: "Gruppe 1", glyph: "group", members: ["/Applications/Foo.app"] };

  it("keeps well-formed rows", () => {
    expect(sanitizeAppGroups([group])).toEqual([group]);
  });

  it("drops rows with a missing id/name, a bad side, duplicate ids, and a group left with zero members", () => {
    const rows = [
      group,
      { ...group, id: "" }, // missing id
      { ...group, id: "g2", name: "" }, // missing name
      { ...group, id: "g3", side: "other" }, // bad side
      { ...group, id: "g1", name: "Dup id" }, // duplicate id, dropped
      { ...group, id: "g4", members: ["", 5, null] }, // every member malformed -> 0 members -> dropped
      "not an object",
      null,
    ];
    expect(sanitizeAppGroups(rows).map((g) => g.id)).toEqual(["g1"]);
  });

  it("dedups a member across groups of the same side, first group wins", () => {
    const rows = [group, { ...group, id: "g2", members: ["/Applications/Foo.app", "/Applications/Bar.app"] }];
    const groups = sanitizeAppGroups(rows);
    expect(groups.find((g) => g.id === "g1")?.members).toEqual(["/Applications/Foo.app"]);
    expect(groups.find((g) => g.id === "g2")?.members).toEqual(["/Applications/Bar.app"]);
  });

  it("does not dedup the same member id across different sides", () => {
    const rows = [
      { ...group, id: "g1", side: "user" as const, members: ["shared"] },
      { ...group, id: "g2", side: "builtin" as const, members: ["shared"] },
    ];
    expect(sanitizeAppGroups(rows).map((g) => g.members)).toEqual([["shared"], ["shared"]]);
  });

  it("returns an empty list for anything that is not an array", () => {
    expect(sanitizeAppGroups(undefined)).toEqual([]);
    expect(sanitizeAppGroups({})).toEqual([]);
  });
});

describe("parseState", () => {
  it("returns null for invalid JSON or a non-object", () => {
    expect(parseState("{ nope")).toBeNull();
    expect(parseState("[1,2]")).toBeNull();
    expect(parseState("null")).toBeNull();
  });

  it("fills defaults for missing sections", () => {
    const s = parseState("{}")!;
    expect(s.version).toBe(1);
    expect(s.settings).toEqual({ theme: "graphite", customCssPath: null });
    expect(s.canvas.instances).toEqual([]);
    expect(s.apps.user).toEqual([]);
    expect(s.apps.groups).toEqual([]);
  });

  it("carries unknown top-level and settings keys and sanitises instances, user apps and groups", () => {
    const s = parseState(
      JSON.stringify({
        version: 1,
        hello: "kept",
        settings: { theme: "ocean", extra: 42, customCssPath: "/tmp/x.css" },
        canvas: { instances: [good, { id: "broken" }] },
        apps: {
          user: [{ path: "/Applications/Foo.app", name: "Foo" }, { path: "" }],
          groups: [
            { id: "g1", side: "user", name: "Gruppe 1", glyph: "group", members: ["/Applications/Foo.app"] },
            { id: "g2", side: "user", name: "" }, // missing name, dropped
          ],
        },
      }),
    )!;
    expect(s.hello).toBe("kept");
    expect(s.settings).toMatchObject({ theme: "ocean", extra: 42, customCssPath: "/tmp/x.css" });
    expect(s.canvas.instances.map((i) => i.id)).toEqual(["a"]);
    expect(s.apps.user).toEqual([{ path: "/Applications/Foo.app", name: "Foo" }]);
    expect(s.apps.groups.map((g) => g.id)).toEqual(["g1"]);
  });

  it("falls back to the default theme and null css path for bad values", () => {
    const s = parseState(JSON.stringify({ settings: { theme: 7, customCssPath: "" } }))!;
    expect(s.settings.theme).toBe("graphite");
    expect(s.settings.customCssPath).toBeNull();
  });
});

describe("settings accessors", () => {
  it("round-trip extra settings through buildState", async () => {
    const { buildState, getSetting, setSetting } = await import("./persist");
    expect(getSetting("secondBrain")).toBeUndefined();
    setSetting("secondBrain", { layout: "circle" });
    expect(getSetting<{ layout: string }>("secondBrain")?.layout).toBe("circle");
    expect(buildState().settings.secondBrain).toEqual({ layout: "circle" });
  });

  it("buildState reflects the userApps store", async () => {
    const { buildState } = await import("./persist");
    const { userApps } = await import("./apps");
    const app = { path: "/Applications/Foo.app", name: "Foo" };
    userApps.set([app]);
    expect(buildState().apps.user).toEqual([app]);
  });

  it("buildState reflects the appGroups store", async () => {
    const { buildState } = await import("./persist");
    const { appGroups } = await import("./appGroups");
    const group = { id: "g1", side: "user" as const, name: "Gruppe 1", glyph: "group", members: ["/Applications/Foo.app"] };
    appGroups.set([group]);
    expect(buildState().apps.groups).toEqual([group]);
  });
});
