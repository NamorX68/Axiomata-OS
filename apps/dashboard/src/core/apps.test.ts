import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { addUserApp, listBuiltinApps, loadUserApps, removeUserApp, userApps } from "./apps";

beforeAll(() => registerBuiltins());
beforeEach(() => loadUserApps([]));

describe("listBuiltinApps", () => {
  it("excludes background, dev, and md-file modules", () => {
    const types = listBuiltinApps().map((a) => a.type);
    expect(types).not.toContain("second-brain");
    expect(types).not.toContain("md-file");
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
});
