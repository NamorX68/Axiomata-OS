import { describe, expect, it } from "vitest";

import { absoluteInside, relativeInside } from "./outputPath";

describe("relativeInside", () => {
  const base = "/Users/me/proj";

  it("resolves absolute paths under the folder and relative ones against it", () => {
    expect(relativeInside("/Users/me/proj/src/a.py", base)).toBe("src/a.py");
    expect(relativeInside("src/a.py", base)).toBe("src/a.py");
    expect(relativeInside("./src/../src/a.py", base)).toBe("src/a.py");
    expect(relativeInside("src\\a.py", base)).toBe("src/a.py");
  });

  it("refuses everything that leaves the folder", () => {
    expect(relativeInside("/etc/passwd", base)).toBeNull();
    expect(relativeInside("/Users/me/proj-other/a.py", base)).toBeNull();
    expect(relativeInside("../x.py", base)).toBeNull();
    expect(relativeInside("src/../../x.py", base)).toBeNull();
    expect(relativeInside("~/x.py", base)).toBeNull();
    expect(relativeInside("/Users/me/proj", base)).toBeNull();
  });
});

describe("absoluteInside", () => {
  it("accepts an absolute path inside the folder", () => {
    expect(absoluteInside("/p/proj/src/main.rs", "/p/proj")).toBe("src/main.rs");
  });

  it("refuses a relative path, which an adapter uses for files it cannot place", () => {
    expect(absoluteInside("library/core/src/ptr/mod.rs", "/p/proj")).toBeNull();
    expect(absoluteInside("src/main.rs", "/p/proj")).toBeNull();
  });

  it("refuses a path outside the folder and a missing one", () => {
    expect(absoluteInside("/Users/x/.rustup/toolchains/s/library/alloc/src/vec/mod.rs", "/p/proj")).toBeNull();
    expect(absoluteInside(null, "/p/proj")).toBeNull();
  });
});
