import { describe, expect, it } from "vitest";

import { resolveMarkdownImages } from "./markdownImages";
import type { WorkspaceImage } from "./backend";

function fakeInvoke(images: Record<string, WorkspaceImage>, calls: string[] = []) {
  return (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    if (cmd !== "read_workspace_image") throw new Error(`unexpected cmd ${cmd}`);
    const rel = (args as { rel: string }).rel;
    calls.push(rel);
    const img = images[rel];
    if (!img) throw new Error(`no such image: ${rel}`);
    return img as unknown as T;
  }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

const PHOTO: WorkspaceImage = { path: "Notes/photo.jpg", mime: "image/jpeg", base64: "Zm9v" };

describe("resolveMarkdownImages", () => {
  it("returns the source unchanged when there are no images", async () => {
    const source = "# Title\n\nJust text, no pictures.";
    expect(await resolveMarkdownImages(source, "Notes/note.md", fakeInvoke({}))).toBe(source);
  });

  it("rewrites a relative image reference to an inline data URI", async () => {
    const source = "See ![a photo](photo.jpg) above.";
    const out = await resolveMarkdownImages(source, "Notes/note.md", fakeInvoke({ "Notes/photo.jpg": PHOTO }));
    expect(out).toBe("See ![a photo](data:image/jpeg;base64,Zm9v) above.");
  });

  it("resolves the target relative to the note's own folder, not the workspace root", async () => {
    const calls: string[] = [];
    const invoke = fakeInvoke({ "Notes/Sub/photo.jpg": PHOTO }, calls);
    await resolveMarkdownImages("![x](Sub/photo.jpg)", "Notes/note.md", invoke);
    expect(calls).toEqual(["Notes/Sub/photo.jpg"]);
  });

  it("leaves already-resolvable http(s): and data: targets untouched, never fetching them", async () => {
    const calls: string[] = [];
    const source = "![a](https://example.com/x.png) ![b](data:image/png;base64,abc)";
    const out = await resolveMarkdownImages(source, "Notes/note.md", fakeInvoke({}, calls));
    expect(out).toBe(source);
    expect(calls).toEqual([]);
  });

  it("fetches the same target only once, even if referenced twice", async () => {
    const calls: string[] = [];
    const invoke = fakeInvoke({ "Notes/photo.jpg": PHOTO }, calls);
    const out = await resolveMarkdownImages("![a](photo.jpg) and again ![b](photo.jpg)", "Notes/note.md", invoke);
    expect(calls).toEqual(["Notes/photo.jpg"]);
    expect(out).toBe("![a](data:image/jpeg;base64,Zm9v) and again ![b](data:image/jpeg;base64,Zm9v)");
  });

  it("leaves a target that fails to resolve exactly as written, not an error", async () => {
    const source = "![missing](nope.jpg)";
    const out = await resolveMarkdownImages(source, "Notes/note.md", fakeInvoke({}));
    expect(out).toBe(source);
  });
});
