import { describe, expect, it } from "vitest";

import { isImagePath, previewKindFor, startsInPreview } from "./fileKinds";

describe("fileKinds (W1, W3)", () => {
  it("knows raster images, and leaves SVG to the text path", () => {
    expect(isImagePath("a/photo.JPG")).toBe(true);
    expect(isImagePath("scan.heic")).toBe(true);
    expect(isImagePath("logo.svg")).toBe(false);
    expect(isImagePath("notes.md")).toBe(false);
  });

  it("gives Markdown, HTML and SVG a rendered view, code none", () => {
    expect(previewKindFor("Notes.markdown")).toBe("markdown");
    expect(previewKindFor("lesson.htm")).toBe("html");
    expect(previewKindFor("logo.svg")).toBe("svg");
    expect(previewKindFor("main.rs")).toBeNull();
  });

  it("starts reading on the rendered view, editing on the source", () => {
    expect(startsInPreview("markdown", "read")).toBe(true);
    expect(startsInPreview("html", "read")).toBe(true);
    expect(startsInPreview("svg", "read")).toBe(false);
    expect(startsInPreview("markdown", "edit")).toBe(false);
    expect(startsInPreview(null, "read")).toBe(false);
    expect(startsInPreview("html", "edit")).toBe(false);
    expect(startsInPreview(null, "edit")).toBe(false);
  });

  it("is case-insensitive about the extension, for both image and preview detection", () => {
    expect(isImagePath("SCAN.TIFF")).toBe(true);
    expect(previewKindFor("REPORT.HTM")).toBe("html");
    expect(previewKindFor("NOTES.MD")).toBe("markdown");
    expect(previewKindFor("LOGO.SVG")).toBe("svg");
  });

  it("recognises every Markdown alias extension, not just .md", () => {
    for (const name of ["a.markdown", "a.mdown", "a.mkd", "a.mkdn"]) {
      expect(previewKindFor(name)).toBe("markdown");
    }
  });

  it("gives no preview and no image kind to a file with no extension at all", () => {
    expect(previewKindFor("README")).toBeNull();
    expect(previewKindFor("Makefile")).toBeNull();
    expect(isImagePath("README")).toBe(false);
  });
});
