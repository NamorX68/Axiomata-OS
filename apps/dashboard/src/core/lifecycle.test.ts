import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { CASCADE_PX, ORIGIN, createInstance, destroyInstance, isPlacedSingleton } from "./lifecycle";
import { registerModule } from "./registry";
import { instances, loadInstances } from "./stores";
import Dummy from "../modules/dummy.svelte";

beforeAll(() => {
  registerBuiltins();
  // A throwaway module type exercising `computeDefaultSize` (see
  // `core/types.ts`'s own doc comment) without depending on Terminal's
  // real canvas-measurement implementation, which jsdom can't render
  // meaningfully (no real font metrics) — this only needs to prove
  // `createInstance` actually calls it and prefers it correctly.
  registerModule({
    type: "test-computed-size",
    title: "Test Computed Size",
    icon: "",
    component: Dummy,
    defaultSize: { w: 111, h: 222 },
    computeDefaultSize: () => ({ w: 999, h: 888 }),
  });
  // A second throwaway type whose `computeDefaultSize` throws — proves the
  // "falls back to `defaultSize`" contract `core/types.ts`'s own doc
  // comment promises actually holds, not just that `createInstance`
  // *prefers* a successful `computeDefaultSize` over `defaultSize` (see
  // the sibling type above) — a real gap architecture review found: `??`
  // alone doesn't catch a thrown error, only a `null`/`undefined` return.
  registerModule({
    type: "test-throwing-computed-size",
    title: "Test Throwing Computed Size",
    icon: "",
    component: Dummy,
    defaultSize: { w: 333, h: 444 },
    computeDefaultSize: () => {
      throw new Error("boom");
    },
  });
});
beforeEach(() => loadInstances([]));

describe("createInstance", () => {
  it("rejects unknown types", () => {
    const r = createInstance("nope");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toMatch(/unknown module type/);
  });

  it("uses the definition's default size and cascades positions", () => {
    // `md-file` is one of the few non-singleton modules — every dashboard
    // tool (memory, skills, routines, mail, calendar, reminders, todo) is now
    // limited to a single instance.
    const a = createInstance("md-file");
    const b = createInstance("md-file");
    expect(a.ok && b.ok).toBe(true);
    if (!a.ok || !b.ok) return;
    expect(a.instance).toMatchObject({ x: ORIGIN.x, y: ORIGIN.y, w: 480, h: 420, z: 1 });
    expect(b.instance).toMatchObject({ x: ORIGIN.x + CASCADE_PX, y: ORIGIN.y + CASCADE_PX, z: 2 });
    expect(get(instances)).toHaveLength(2);
  });

  it("honours overrides", () => {
    const r = createInstance("md-file", { x: 5, y: 6, config: { path: "a.md" } });
    expect(r.ok && r.instance).toMatchObject({ x: 5, y: 6, config: { path: "a.md" } });
  });

  it("blocks a second singleton until the first is removed", () => {
    const first = createInstance("dummy-singleton");
    expect(first.ok).toBe(true);
    expect(isPlacedSingleton("dummy-singleton")).toBe(true);
    const second = createInstance("dummy-singleton");
    expect(second.ok).toBe(false);
    if (!second.ok) expect(second.reason).toMatch(/only one instance/);
    if (first.ok) expect(destroyInstance(first.instance.id)).toBe(true);
    expect(isPlacedSingleton("dummy-singleton")).toBe(false);
    expect(createInstance("dummy-singleton").ok).toBe(true);
  });

  it("destroyInstance reports whether anything was removed", () => {
    expect(destroyInstance("missing")).toBe(false);
  });

  it("prefers computeDefaultSize over the static defaultSize when present", () => {
    const r = createInstance("test-computed-size");
    expect(r.ok && r.instance).toMatchObject({ w: 999, h: 888 });
  });

  it("an explicit override still wins over computeDefaultSize", () => {
    const r = createInstance("test-computed-size", { w: 42, h: 43 });
    expect(r.ok && r.instance).toMatchObject({ w: 42, h: 43 });
  });

  it("falls back to the static defaultSize when computeDefaultSize throws, rather than propagating", () => {
    const r = createInstance("test-throwing-computed-size");
    expect(r.ok && r.instance).toMatchObject({ w: 333, h: 444 });
  });
});
