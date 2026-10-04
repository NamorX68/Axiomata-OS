/**
 * `viShared` is a module-level singleton (one `ViShared` for the whole app), so
 * every test resets the module registry and re-imports it fresh — otherwise a
 * later test would see the previous test's instance and its clipboard-setting
 * subscription (D17, V3, V4).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ClipboardPending } from "../editor/vi/registers";

vi.mock("../core/backend", () => ({ invokeBackend: vi.fn() }));

async function load() {
  const { invokeBackend } = await import("../core/backend");
  // Every call answers unless a test says otherwise (a yank writes the clipboard twice: "+ and the unnamed one);
  // the remembered Vi state reads as an empty file.
  vi.mocked(invokeBackend).mockImplementation(async (cmd: string) =>
    cmd === "get_editor_vi_state" ? { json: '{"version":1}', recovered_backup: null } : undefined,
  );
  const { editorSettings, DEFAULT_EDITOR_SETTINGS } = await import("./editorSettings");
  const { viShared } = await import("./viShared");
  // The class as the freshly loaded modules know it — `resetModules` makes a new one each time.
  const { ClipboardPending: Pending } = await import("../editor/vi/registers");
  return { invokeBackend: vi.mocked(invokeBackend), editorSettings, DEFAULT_EDITOR_SETTINGS, viShared, Pending };
}

beforeEach(() => {
  vi.resetModules();
});

describe("viShared (D17, V3, V4)", () => {
  it("creates one ViShared and reuses it on later calls", async () => {
    const { viShared } = await load();
    expect(viShared()).toBe(viShared());
  });

  it("starts with the unnamed register as the Mac clipboard (the default, 'shared')", async () => {
    const { viShared } = await load();
    expect(viShared().registers.shared).toBe(true);
  });

  it("follows the viClipboard setting as it changes, for the already-created ViShared", async () => {
    const { viShared, editorSettings, DEFAULT_EDITOR_SETTINGS } = await load();
    const shared = viShared();
    editorSettings.set({ ...DEFAULT_EDITOR_SETTINGS, viClipboard: "separate" });
    expect(shared.registers.shared).toBe(false);
    editorSettings.set({ ...DEFAULT_EDITOR_SETTINGS, viClipboard: "shared" });
    expect(shared.registers.shared).toBe(true);
  });

  it("subscribes to the setting even for a ViShared created after settings already changed", async () => {
    // The store is a Svelte writable: subscribing after a `.set()` still delivers the current value.
    const { viShared, editorSettings, DEFAULT_EDITOR_SETTINGS } = await load();
    editorSettings.set({ ...DEFAULT_EDITOR_SETTINGS, viClipboard: "separate" });
    expect(viShared().registers.shared).toBe(false);
  });

  it("reads the Mac clipboard through clipboard_read", async () => {
    const { viShared, invokeBackend, Pending } = await load();
    const shared = viShared();
    invokeBackend.mockResolvedValueOnce("clipped text");
    let pending: Promise<string> | null = null;
    try {
      shared.registers.get("+");
    } catch (err) {
      expect(err).toBeInstanceOf(Pending);
      pending = (err as ClipboardPending).text;
    }
    expect(invokeBackend).toHaveBeenCalledWith("clipboard_read");
    await expect(pending).resolves.toBe("clipped text");
  });

  it("resolves to an empty string when clipboard_read fails, instead of rejecting", async () => {
    const { viShared, invokeBackend } = await load();
    const shared = viShared();
    invokeBackend.mockRejectedValueOnce(new Error("no pasteboard access"));
    let pending: Promise<string> | null = null;
    try {
      shared.registers.get("*");
    } catch (err) {
      pending = (err as ClipboardPending).text;
    }
    await expect(pending).resolves.toBe("");
  });

  it("writes a yank to the Mac clipboard through clipboard_write, without waiting for it", async () => {
    const { viShared, invokeBackend } = await load();
    invokeBackend.mockResolvedValueOnce(undefined);
    viShared().registers.put("+", { text: "copied", kind: "char" }, "yank");
    expect(invokeBackend).toHaveBeenCalledWith("clipboard_write", { text: "copied" });
  });

  it("does not throw when clipboard_write's own promise rejects (fire-and-forget)", async () => {
    const { viShared, invokeBackend } = await load();
    invokeBackend.mockRejectedValueOnce(new Error("no pasteboard access"));
    expect(() => viShared().registers.put("+", { text: "copied", kind: "char" }, "yank")).not.toThrow();
  });
});

describe("remembering across restarts (V4)", () => {
  it("restores the named registers and the last search from editor-vi.json when first made", async () => {
    const { viShared, invokeBackend } = await load();
    const saved = {
      version: 1,
      registers: { a: { text: "macro", kind: "char" } },
      lastSearch: { pattern: "foo", backward: false },
    };
    invokeBackend.mockImplementation(async (cmd: string) =>
      cmd === "get_editor_vi_state" ? { json: JSON.stringify(saved), recovered_backup: null } : undefined,
    );
    const shared = viShared();
    await vi.waitFor(() => expect(shared.registers.get("a")?.text).toBe("macro"));
    expect(shared.search.last?.pattern).toBe("foo");
  });

  it("writes the state once the keys settle, and not before the file was read", async () => {
    vi.useFakeTimers();
    try {
      const { viShared, invokeBackend } = await load();
      const { viStateChanged } = await import("./viShared");
      const shared = viShared();
      await vi.waitFor(() => expect(invokeBackend).toHaveBeenCalledWith("get_editor_vi_state"));
      await Promise.resolve();
      shared.registers.set("q", { text: "dd", kind: "char" });
      viStateChanged();
      viStateChanged();
      await vi.advanceTimersByTimeAsync(1500);
      const saves = invokeBackend.mock.calls.filter(([cmd]) => cmd === "save_editor_vi_state");
      expect(saves).toHaveLength(1);
      const json = JSON.parse((saves[0][1] as { json: string }).json);
      expect(json.registers.q).toEqual({ text: "dd", kind: "char" });
    } finally {
      vi.useRealTimers();
    }
  });

  it("still enables saving once the keys settle after get_editor_vi_state rejects", async () => {
    vi.useFakeTimers();
    try {
      const { invokeBackend } = await load();
      invokeBackend.mockImplementation(async (cmd: string) => {
        if (cmd === "get_editor_vi_state") throw new Error("no file yet");
        return undefined;
      });
      const { viShared, viStateChanged } = await import("./viShared");
      const shared = viShared();
      await vi.waitFor(() => expect(invokeBackend).toHaveBeenCalledWith("get_editor_vi_state"));
      // Let the rejected promise's `.catch`/`.finally` run (a couple of microtask hops).
      await Promise.resolve();
      await Promise.resolve();
      shared.registers.set("q", { text: "dd", kind: "char" });
      viStateChanged();
      await vi.advanceTimersByTimeAsync(1500);
      const saves = invokeBackend.mock.calls.filter(([cmd]) => cmd === "save_editor_vi_state");
      expect(saves).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not throw and still enables saving when the remembered state is not valid JSON", async () => {
    vi.useFakeTimers();
    try {
      const { invokeBackend } = await load();
      invokeBackend.mockImplementation(async (cmd: string) =>
        cmd === "get_editor_vi_state" ? { json: "{ not json", recovered_backup: null } : undefined,
      );
      const { viShared, viStateChanged } = await import("./viShared");
      let shared: ReturnType<typeof viShared>;
      expect(() => {
        shared = viShared();
      }).not.toThrow();
      await vi.waitFor(() => expect(invokeBackend).toHaveBeenCalledWith("get_editor_vi_state"));
      await Promise.resolve();
      await Promise.resolve();
      shared!.registers.set("q", { text: "dd", kind: "char" });
      viStateChanged();
      await vi.advanceTimersByTimeAsync(1500);
      const saves = invokeBackend.mock.calls.filter(([cmd]) => cmd === "save_editor_vi_state");
      expect(saves).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });
});
