import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { CASCADE_PX, ORIGIN, createInstance, destroyInstance, isPlacedSingleton } from "./lifecycle";
import { instances, loadInstances } from "./stores";

beforeAll(() => registerBuiltins());
beforeEach(() => loadInstances([]));

describe("createInstance", () => {
  it("rejects unknown types", () => {
    const r = createInstance("nope");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toMatch(/unknown module type/);
  });

  it("uses the definition's default size and cascades positions", () => {
    const a = createInstance("memory-status");
    const b = createInstance("memory-status");
    expect(a.ok && b.ok).toBe(true);
    if (!a.ok || !b.ok) return;
    expect(a.instance).toMatchObject({ x: ORIGIN.x, y: ORIGIN.y, w: 360, h: 150, z: 1 });
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
});
