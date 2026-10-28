import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";

import { invokeBackend } from "./backend";
import { clearWallpaper, imageType, loadWallpaper, pickWallpaper, startWallpaper, wallpaper } from "./wallpaper";

vi.mock("./backend", () => ({ invokeBackend: vi.fn() }));
const invoke = vi.mocked(invokeBackend);

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

describe("imageType", () => {
  it("knows the pictures Rust stores by their first bytes", () => {
    expect(imageType(PNG)).toBe("image/png");
    expect(imageType(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe("image/jpeg");
    expect(imageType(new TextEncoder().encode("RIFF\0\0\0\0WEBPVP8 "))).toBe("image/webp");
    expect(imageType(new TextEncoder().encode("GIF89a"))).toBeNull();
    expect(imageType(new TextEncoder().encode("<svg"))).toBeNull();
    expect(imageType(new Uint8Array())).toBeNull();
  });
});

describe("the background image", () => {
  const root = document.documentElement;
  URL.createObjectURL = vi.fn(() => "blob:image");
  URL.revokeObjectURL = vi.fn();

  afterEach(() => {
    invoke.mockReset();
    wallpaper.set(null);
    root.style.removeProperty("--ax-wallpaper");
  });

  it("shows the stored image as a blob and puts it on <html>", async () => {
    invoke.mockResolvedValue(PNG.buffer);
    const stop = startWallpaper();
    await vi.waitFor(() => expect(get(wallpaper)).toBe("blob:image"));
    expect(root.style.getPropertyValue("--ax-wallpaper")).toBe('url("blob:image")');
    stop();
  });

  it("shows the theme's ground when nothing is stored", async () => {
    wallpaper.set("blob:old");
    invoke.mockResolvedValue(new ArrayBuffer(0));
    await loadWallpaper();
    expect(get(wallpaper)).toBeNull();
  });

  it("keeps the image on show when a read fails", async () => {
    wallpaper.set("blob:shown");
    invoke.mockRejectedValue(new Error("busy"));
    await loadWallpaper();
    expect(get(wallpaper)).toBe("blob:shown");
  });

  it("reads the image again after a pick, and not after a cancelled one", async () => {
    invoke.mockResolvedValueOnce(false);
    expect(await pickWallpaper()).toBe(false);
    expect(invoke).toHaveBeenCalledTimes(1);
    invoke.mockResolvedValueOnce(true).mockResolvedValueOnce(PNG.buffer);
    expect(await pickWallpaper()).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith("wallpaper_read");
    expect(get(wallpaper)).toBe("blob:image");
  });

  it("goes back to the theme's ground when cleared, and lets the old blob go", async () => {
    wallpaper.set("blob:old");
    invoke.mockResolvedValue(undefined);
    await clearWallpaper();
    expect(invoke).toHaveBeenCalledWith("wallpaper_clear");
    expect(get(wallpaper)).toBeNull();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:old");
  });
});
