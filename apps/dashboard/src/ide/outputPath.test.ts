import { describe, expect, it } from "vitest";

import { relativeInside } from "./outputPath";

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
