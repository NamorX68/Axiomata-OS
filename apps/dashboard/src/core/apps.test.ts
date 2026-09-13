import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { addUserApp, listBuiltinApps, loadUserApps, removeUserApp, setUserAppGlyph, userApps } from "./apps";
import { appGroups, createGroup, groupFor, loadAppGroups } from "./appGroups";

beforeAll(() => registerBuiltins());
beforeEach(() => {
  loadUserApps([]);
  loadAppGroups([]);
});

describe("listBuiltinApps", () => {
  it("excludes background, dev, md-file, and terminal modules", () => {
    const types = listBuiltinApps().map((a) => a.type);
    expect(types).not.toContain("second-brain");
    expect(types).not.toContain("md-file");
    expect(types).not.toContain("terminal");
    expect(types).not.toContain("dummy");
    expect(types).not.toContain("dummy-singleton");
  });

  it("carries title through from the registry", () => {
    const memoryStatus = listBuiltinApps().find((a) => a.type === "memory-status");
    expect(memoryStatus).toMatchObject({ title: "Memory" });
  });

  it("preserves registry order for the remaining tools", () => {
    // Every one of these is a singleton (see `apps.ts`'s doc comment) — a
    // regression here would also silently reopen the "what does a repeat
    // ring click on a non-singleton do" question the plan deliberately
    // sidesteps for v1.
    expect(listBuiltinApps().map((a) => a.type)).toEqual([
      "memory-status",
      "skills-deck",
      "routines-board",
      "todo",
      "calendar",
      "reminders",
      "mail",
    ]);
  });
});

describe("userApps store", () => {
  it("addUserApp appends, ignoring a duplicate path", () => {
    addUserApp({ path: "/Applications/Foo.app", name: "Foo" });
    addUserApp({ path: "/Applications/Foo.app", name: "Foo (dup)" });
    expect(get(userApps)).toEqual([{ path: "/Applications/Foo.app", name: "Foo" }]);
  });

  it("removeUserApp removes exactly the one path, others untouched", () => {
    addUserApp({ path: "/Applications/Foo.app", name: "Foo" });
    addUserApp({ path: "/Applications/Bar.app", name: "Bar" });
    removeUserApp("/Applications/Foo.app");
    expect(get(userApps)).toEqual([{ path: "/Applications/Bar.app", name: "Bar" }]);
  });

  it("removing a path that isn't present is a no-op", () => {
    addUserApp({ path: "/Applications/Bar.app", name: "Bar" });
    removeUserApp("/Applications/Ghost.app");
    expect(get(userApps)).toEqual([{ path: "/Applications/Bar.app", name: "Bar" }]);
  });

  it("removeUserApp also drops the app from whatever group it was in, dissolving a group left empty", () => {
    addUserApp({ path: "/Applications/Foo.app", name: "Foo" });
    const group = createGroup("user", "/Applications/Foo.app");
    removeUserApp("/Applications/Foo.app");
    expect(groupFor("user", "/Applications/Foo.app")).toBeUndefined();
    expect(get(appGroups).find((g) => g.id === group.id)).toBeUndefined();
  });

  it("setUserAppGlyph sets or clears the icon override for exactly one app", () => {
    addUserApp({ path: "/Applications/Foo.app", name: "Foo" });
    addUserApp({ path: "/Applications/Bar.app", name: "Bar" });
    setUserAppGlyph("/Applications/Foo.app", "wrench");
    expect(get(userApps)).toEqual([
      { path: "/Applications/Foo.app", name: "Foo", glyph: "wrench" },
      { path: "/Applications/Bar.app", name: "Bar" },
    ]);
    setUserAppGlyph("/Applications/Foo.app", undefined);
    expect(get(userApps)[0]).toEqual({ path: "/Applications/Foo.app", name: "Foo", glyph: undefined });
  });

  it("setUserAppGlyph on a path that isn't present is a no-op", () => {
    addUserApp({ path: "/Applications/Bar.app", name: "Bar" });
    setUserAppGlyph("/Applications/Ghost.app", "wrench");
    expect(get(userApps)).toEqual([{ path: "/Applications/Bar.app", name: "Bar" }]);
  });
});
