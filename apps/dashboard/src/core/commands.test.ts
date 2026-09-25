import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { helpText, isCommand, route, runCommand } from "./commands";
import { activeTheme, instances, loadInstances } from "./stores";
import { staged } from "./staging";

beforeAll(() => registerBuiltins());
beforeEach(() => {
  loadInstances([]);
  staged.set([]);
});

describe("route", () => {
  it("classifies chat, instruct and registered commands", () => {
    expect(route("   ")).toBeNull();
    expect(route("hello there")).toEqual({ kind: "chat", message: "hello there" });
    expect(route("/schreib Milch auf die Liste")).toEqual({
      kind: "instruct",
      message: "schreib Milch auf die Liste",
    });
    expect(route("/THEME ocean")).toEqual({ kind: "command", name: "theme", args: ["ocean"] });
    expect(route("/memory-status status")).toEqual({
      kind: "command",
      name: "memory-status",
      args: ["status"],
    });
  });

  it("knows shell commands and module types", () => {
    expect(isCommand("help")).toBe(true);
    expect(isCommand("skills-deck")).toBe(true);
    expect(isCommand("nope")).toBe(false);
  });
});

describe("runCommand", () => {
  it("/help returns the command list as detail", async () => {
    const r = await runCommand("help", []);
    expect(r.ok).toBe(true);
    expect(r.detail).toContain("/add <type>");
  });

  it("/add and /remove manage tiles", async () => {
    expect((await runCommand("add", ["memory-status"])).ok).toBe(true);
    expect(get(instances)).toHaveLength(1);
    const bad = await runCommand("add", ["nope"]);
    expect(bad.ok).toBe(false);
    expect(bad.message).toMatch(/unknown module type/);
    expect((await runCommand("remove", ["memory-status"])).ok).toBe(true);
    expect(get(instances)).toHaveLength(0);
    expect((await runCommand("remove", ["memory-status"])).ok).toBe(false);
  });

  it("/theme switches known themes only", async () => {
    document.documentElement.dataset.theme = "graphite";
    expect((await runCommand("theme", ["ocean"])).ok).toBe(true);
    expect(get(activeTheme)).toBe("ocean");
    expect(document.documentElement.dataset.theme).toBe("ocean");
    const bad = await runCommand("theme", ["neon"]);
    expect(bad.ok).toBe(false);
    expect(get(activeTheme)).toBe("ocean");
  });

  it("/open stages a markdown panel", async () => {
    expect((await runCommand("open", [])).ok).toBe(false);
    expect((await runCommand("open", ["notes/inbox.md"])).ok).toBe(true);
    expect(get(staged)).toMatchObject([{ type: "file", config: { path: "notes/inbox.md", mode: "read" } }]);
  });

  it("/newfile creates an empty file and opens it in edit mode, not read mode", async () => {
    expect((await runCommand("newfile", [])).ok).toBe(false);
    const r = await runCommand("newfile", ["notes/new.md"]);
    expect(r.ok).toBe(true);
    expect(get(staged)).toMatchObject([{ type: "file", config: { path: "notes/new.md", mode: "edit" } }]);
  });

  it("/newfile reports the backend's failure instead of opening a panel", async () => {
    const spy = vi.spyOn(await import("./backend"), "invokeBackend").mockRejectedValueOnce(new Error("disk full"));
    const r = await runCommand("newfile", ["notes/broken.md"]);
    expect(r.ok).toBe(false);
    expect(r.message).toMatch(/disk full/);
    expect(get(staged)).toHaveLength(0);
    spy.mockRestore();
  });

  it("/help's /add <type> list names every placeable module type, read when asked", () => {
    // Built when `/help` runs, not when the module loads — the builtins register after it is imported.
    expect(helpText()).toContain("`memory-status`");
    expect(helpText()).not.toContain("`file`");
  });

  it("module actions need a mounted instance and valid JSON params", async () => {
    const none = await runCommand("memory-status", ["status"]);
    expect(none.ok).toBe(false);
    expect(none.message).toMatch(/\/add memory-status/);
    await runCommand("add", ["memory-status"]);
    const usage = await runCommand("memory-status", []);
    expect(usage.ok).toBe(false);
    expect(usage.message).toMatch(/actions: `sync`, `status`/);
    const badJson = await runCommand("memory-status", ["status", "{nope"]);
    expect(badJson.ok).toBe(false);
    const ok = await runCommand("memory-status", ["status"]);
    expect(ok.ok).toBe(true);
    expect(ok.detail).toContain("tracked_files");
    const unknown = await runCommand("memory-status", ["explode"]);
    expect(unknown.ok).toBe(false);
    expect(unknown.message).toMatch(/no action "explode"/);
  });

  it("unknown commands are reported", async () => {
    const r = await runCommand("frobnicate", []);
    expect(r.ok).toBe(false);
  });
});
