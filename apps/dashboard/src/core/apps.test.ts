import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import {
  addUserApp,
  hiddenBuiltins,
  hideBuiltinApp,
  listAllRingEligibleBuiltins,
  listBuiltinApps,
  loadHiddenBuiltins,
  loadUserApps,
  removeUserApp,
  setUserAppGlyph,
  showBuiltinApp,
  userApps,
} from "./apps";
import { appGroups, createGroup, groupFor, loadAppGroups } from "./appGroups";

beforeAll(() => registerBuiltins());
beforeEach(() => {
  loadUserApps([]);
  loadAppGroups([]);
  loadHiddenBuiltins([]);
});

describe("listAllRingEligibleBuiltins / listBuiltinApps", () => {
  it("excludes background, dev, and md-file modules", () => {
    const types = listAllRingEligibleBuiltins().map((a) => a.type);
    expect(types).not.toContain("second-brain");
    expect(types).not.toContain("md-file");
    expect(types).not.toContain("dummy");
    expect(types).not.toContain("dummy-singleton");
  });

  it("includes terminal — a non-singleton builtin is still ring-eligible", () => {
    expect(listAllRingEligibleBuiltins().map((a) => a.type)).toContain("terminal");
    expect(listBuiltinApps().map((a) => a.type)).toContain("terminal");
  });

  it("carries title through from the registry", () => {
    const memoryStatus = listBuiltinApps().find((a) => a.type === "memory-status");
    expect(memoryStatus).toMatchObject({ title: "Memory" });
  });

  it("preserves registry order", () => {
    expect(listAllRingEligibleBuiltins().map((a) => a.type)).toEqual([
      "memory-status",
      "skills-deck",
      "routines-board",
      "todo",
      "calendar",
      "reminders",
      "mail",
      "terminal",
    ]);
  });

  it("listBuiltinApps excludes a hidden type, listAllRingEligibleBuiltins still includes it", () => {
    hideBuiltinApp("mail");
    expect(listBuiltinApps().map((a) => a.type)).not.toContain("mail");
    expect(listAllRingEligibleBuiltins().map((a) => a.type)).toContain("mail");
  });
});

describe("hiddenBuiltins store", () => {
  it("hideBuiltinApp adds, ignoring a duplicate; showBuiltinApp removes", () => {
    hideBuiltinApp("mail");
    hideBuiltinApp("mail");
    expect(get(hiddenBuiltins)).toEqual(["mail"]);
    showBuiltinApp("mail");
    expect(get(hiddenBuiltins)).toEqual([]);
  });

  it("showBuiltinApp on a type that isn't hidden is a no-op", () => {
    hideBuiltinApp("mail");
    showBuiltinApp("todo");
    expect(get(hiddenBuiltins)).toEqual(["mail"]);
  });

  it("hiding one type leaves the others visible", () => {
    hideBuiltinApp("mail");
    const types = listBuiltinApps().map((a) => a.type);
    expect(types).toContain("todo");
    expect(types).not.toContain("mail");
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
