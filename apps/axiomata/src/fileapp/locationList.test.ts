import { describe, expect, it } from "vitest";

import type { Location } from "../editor/lsp/client";
import { buildLocationList, MAX_FILES_READ } from "./locationList";

const place = (path: string, line: number, col: number, endCol?: number): Location => ({
  uri: `file://${path}`,
  at: { line, col },
  ...(endCol === undefined ? {} : { end: { line, col: endCol } }),
});

/** Files under `/p` belong to the root `project:1`; the rest to the server's read-only root. */
const toFile = (l: Location) => {
  const path = l.uri.slice("file://".length);
  if (path.startsWith("/p/")) return { root: "project:1", rel: path.slice(3) };
  if (path.startsWith("/")) return { root: "lsp:1", rel: path };
  return null;
};

describe("buildLocationList (ED6.3)", () => {
  it("groups by file in the server's order, sorts within a file, and marks the symbol", async () => {
    const texts: Record<string, string> = {
      "project:1\0b.rs": "fn b() {\n    parse(x);\n}",
      "project:1\0a.rs": "use parse;\nparse(y);",
    };
    const reads: string[] = [];
    const list = await buildLocationList(
      "References to `parse`",
      [place("/p/b.rs", 1, 4, 9), place("/p/a.rs", 1, 0, 5), place("/p/a.rs", 0, 4, 9)],
      toFile,
      async (root, rel) => {
        reads.push(rel);
        return texts[`${root}\0${rel}`];
      },
    );
    expect(list.count).toBe(3);
    expect(list.files.map((f) => f.rel)).toEqual(["b.rs", "a.rs"]);
    expect(reads.sort()).toEqual(["a.rs", "b.rs"]);
    expect(list.files[1].matches.map((m) => m.line)).toEqual([0, 1]);
    expect(list.files[0].matches[0]).toEqual({ line: 1, col: 4, end: 9, text: "    parse(x);", from: 0 });
  });

  it("keeps places of a file it cannot read, skips what is no file, and reads at most so many files", async () => {
    const many = Array.from({ length: MAX_FILES_READ + 2 }, (_, i) => place(`/p/f${i}.rs`, 0, 0));
    let reads = 0;
    const list = await buildLocationList(
      "t",
      [place("/opt/lib.rs", 3, 1), { uri: "untitled:1", at: { line: 0, col: 0 } }, ...many],
      toFile,
      async (root) => {
        reads++;
        if (root.startsWith("lsp:")) throw new Error("gone");
        return "x";
      },
    );
    expect(list.files[0]).toMatchObject({ root: "lsp:1", rel: "/opt/lib.rs" });
    expect(list.files[0].matches[0]).toMatchObject({ line: 3, text: "" });
    expect(list.files).toHaveLength(MAX_FILES_READ + 3);
    expect(reads).toBe(MAX_FILES_READ);
  });

  it("shows a window of a long line around the place", async () => {
    const line = "a".repeat(500) + "target" + "b".repeat(500);
    const list = await buildLocationList("t", [place("/p/x.rs", 0, 500, 506)], toFile, async () => line);
    const m = list.files[0].matches[0];
    expect(m.from).toBeGreaterThan(0);
    expect(m.text.slice(m.col - m.from, m.end - m.from)).toBe("target");
  });
});
