import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import { registerBuiltins } from "../modules";
import { createInstance } from "./lifecycle";
import { invokeAction, manifest, registerModule, registerShellAction, SHELL_INSTANCE } from "./registry";
import { loadInstances } from "./stores";
import Dummy from "../modules/dummy.svelte";

beforeAll(() => registerBuiltins());
beforeEach(() => loadInstances([]));

describe("registerShellAction", () => {
  it("throws for a duplicate action name, the same way registerModule guards duplicate types", () => {
    // `openFile` is already registered by `registerBuiltins()` (`modules/index.ts`).
    expect(() =>
      registerShellAction({
        name: "openFile",
        description: "a duplicate registration",
        params: { type: "object", properties: {} },
        run: async () => undefined,
      }),
    ).toThrow(/shell action "openFile" is already registered/);
  });
});

describe("registerModule", () => {
  it("throws for a duplicate module type", () => {
    expect(() =>
      registerModule({
        type: "dummy",
        title: "Duplicate Dummy",
        icon: "",
        component: Dummy,
        defaultSize: { w: 1, h: 1 },
      }),
    ).toThrow(/module type "dummy" is already registered/);
  });
});

describe("manifest", () => {
  it("lists the shell first, then mounted instances that declare at least one action", () => {
    const r = createInstance("dummy");
    expect(r.ok).toBe(true);
    if (!r.ok) return;

    const entries = manifest();
    expect(entries[0]).toMatchObject({ instance_id: SHELL_INSTANCE, type: "shell", title: "Shell" });
    expect(entries[0].actions.map((a) => a.name)).toContain("openFile");

    const dummyEntry = entries.find((e) => e.instance_id === r.instance.id);
    expect(dummyEntry).toMatchObject({ type: "dummy", actions: [{ name: "ping" }] });
  });

  it("omits a mounted instance whose module declares no actions", () => {
    const r = createInstance("dummy-singleton");
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(manifest().some((e) => e.instance_id === r.instance.id)).toBe(false);
  });
});

describe("invokeAction", () => {
  it("propagates a failing shell action's rejection to the caller", async () => {
    await expect(invokeAction(SHELL_INSTANCE, "openFile", { path: "" })).rejects.toThrow(/needs a path/);
  });

  it("rejects an unknown shell action by name", async () => {
    await expect(invokeAction(SHELL_INSTANCE, "nope", {})).rejects.toThrow(/the shell has no action "nope"/);
  });

  it("propagates a failing module action's rejection to the caller", async () => {
    registerModule({
      type: "test-failing-action",
      title: "Failing",
      icon: "",
      component: Dummy,
      defaultSize: { w: 1, h: 1 },
      actions: [
        {
          name: "explode",
          description: "always fails",
          params: { type: "object", properties: {} },
          run: async () => {
            throw new Error("boom");
          },
        },
      ],
    });
    const r = createInstance("test-failing-action");
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    await expect(invokeAction(r.instance.id, "explode", {})).rejects.toThrow("boom");
  });

  it("rejects an unknown instance id", async () => {
    await expect(invokeAction("ghost", "ping", {})).rejects.toThrow(/no module instance "ghost"/);
  });

  it("rejects an unknown action on a known instance", async () => {
    const r = createInstance("dummy");
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    await expect(invokeAction(r.instance.id, "nope", {})).rejects.toThrow(/has no action "nope"/);
  });
});
