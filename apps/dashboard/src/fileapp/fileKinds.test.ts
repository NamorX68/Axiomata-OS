import { describe, expect, it } from "vitest";

import { headOf, isImagePath, previewKindFor, startsInPreview } from "./fileKinds";

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

describe("headOf (W15)", () => {
  it("keeps a short text whole", () => {
    expect(headOf("a\nb", 3)).toEqual({ text: "a\nb", cut: false });
    expect(headOf("", 3)).toEqual({ text: "", cut: false });
  });

  it("cuts after the last line kept, and says so", () => {
    expect(headOf("1\n2\n3\n4", 2)).toEqual({ text: "1\n2", cut: true });
  });

  it("does not call a trailing line break a cut", () => {
    expect(headOf("1\n2\n", 2)).toEqual({ text: "1\n2", cut: false });
  });

  it("keeps a text that is exactly maxLines lines, with no trailing newline", () => {
    expect(headOf("1\n2", 2)).toEqual({ text: "1\n2", cut: false });
  });

  it("keeps a text that is exactly maxLines lines, with a trailing newline", () => {
    expect(headOf("1\n2\n", 2)).toEqual({ text: "1\n2", cut: false });
  });

  it("keeps one line whole when maxLines is 1", () => {
    expect(headOf("only", 1)).toEqual({ text: "only", cut: false });
  });

  it("cuts after the first line when maxLines is 1 and more follows", () => {
    expect(headOf("1\n2\n3", 1)).toEqual({ text: "1", cut: true });
  });

  it("treats CRLF line endings as ordinary text around the \\n it splits on", () => {
    // headOf only looks for "\n"; a "\r" immediately before it is kept as part
    // of the returned text, same as it is part of the source line itself.
    expect(headOf("1\r\n2\r\n3", 2)).toEqual({ text: "1\r\n2\r", cut: true });
  });

  it("keeps a very long single line whole when it is under maxLines", () => {
    const longLine = "x".repeat(10_000);
    expect(headOf(longLine, 3)).toEqual({ text: longLine, cut: false });
  });

  it("returns an empty head for maxLines 0, not the text minus its last character", () => {
    expect(headOf("abc", 0)).toEqual({ text: "", cut: true });
    expect(headOf("", 0)).toEqual({ text: "", cut: false });
  });
});
