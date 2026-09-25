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
  // Every call answers unless a test says otherwise (a yank writes the clipboard twice: "+ and the unnamed one).
  vi.mocked(invokeBackend).mockResolvedValue(undefined);
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
    invokeBackend.mockResolvedValueOnce("clipped text");
    let pending: Promise<string> | null = null;
    try {
      viShared().registers.get("+");
    } catch (err) {
      expect(err).toBeInstanceOf(Pending);
      pending = (err as ClipboardPending).text;
    }
    expect(invokeBackend).toHaveBeenCalledWith("clipboard_read");
    await expect(pending).resolves.toBe("clipped text");
  });

  it("resolves to an empty string when clipboard_read fails, instead of rejecting", async () => {
    const { viShared, invokeBackend } = await load();
    invokeBackend.mockRejectedValueOnce(new Error("no pasteboard access"));
    let pending: Promise<string> | null = null;
    try {
      viShared().registers.get("*");
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
