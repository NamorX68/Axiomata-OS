import { describe, expect, it } from "vitest";

import { isFresh, parseQuery, rankFiles, scorePath } from "./quickOpen";

const files = (...rels: string[]) => rels.map((rel) => ({ root: "p", rel }));

describe("parseQuery", () => {
  it("splits off a line to jump to", () => {
    expect(parseQuery("main.rs:12")).toEqual({ text: "main.rs", line: 11 });
    expect(parseQuery(" notes ")).toEqual({ text: "notes", line: null });
    expect(parseQuery("a:0")).toEqual({ text: "a", line: 0 });
  });

  it("takes the last colon as the line separator when the text itself has one", () => {
    expect(parseQuery("a:b:12")).toEqual({ text: "a:b", line: 11 });
  });

  it("accepts a bare line with an empty file name", () => {
    expect(parseQuery(":12")).toEqual({ text: "", line: 11 });
  });

  it("still finds the line after trailing spaces", () => {
    expect(parseQuery("main.rs:12   ")).toEqual({ text: "main.rs", line: 11 });
  });

  it("does not treat a space before the line number as one", () => {
    expect(parseQuery("main.rs: 12")).toEqual({ text: "main.rs: 12", line: null });
  });
});

describe("scorePath", () => {
  it("finds the letters in order, not necessarily together", () => {
    expect(scorePath("mnrs", "src/main.rs")).not.toBeNull();
    expect(scorePath("srm", "src/main.rs")?.positions).toEqual([0, 1, 4]);
    expect(scorePath("xyz", "src/main.rs")).toBeNull();
  });

  it("ranks a match in the file name above one across folders", () => {
    const name = scorePath("main", "src/main.rs")!.score;
    const folders = scorePath("main", "m/a/i/n.rs")!.score;
    expect(name).toBeGreaterThan(folders);
  });

  it("prefers word starts and runs", () => {
    expect(scorePath("fe", "FileEditor.svelte")!.score).toBeGreaterThan(scorePath("fe", "safely.ts")!.score);
  });

  it("ignores spaces typed into the query", () => {
    expect(scorePath("m a i n", "src/main.rs")).toEqual(scorePath("main", "src/main.rs"));
  });

  it("matches case-insensitively", () => {
    expect(scorePath("MAIN", "src/main.rs")).toEqual(scorePath("main", "src/main.rs"));
  });

  it("finds camelCase word starts inside the file name", () => {
    // Both `F` and `E` are word starts in `FileEditor.svelte`.
    expect(scorePath("fe", "FileEditor.svelte")!.positions).toEqual([0, 4]);
  });

  it("finds nothing for a query longer than the path", () => {
    expect(scorePath("verylongquery", "a.md")).toBeNull();
  });

  it("gives positions across folders when the file name alone does not match", () => {
    expect(scorePath("sdm", "src/deep/main.rs")!.positions).toEqual([0, 4, 9]);
  });
});

describe("rankFiles", () => {
  it("puts the best match first, the shorter path on a tie", () => {
    const ranked = rankFiles("main", files("lib/domain.rs", "src/main.rs", "src/deep/main.rs"), []);
    expect(ranked.map((r) => r.rel)).toEqual(["src/main.rs", "src/deep/main.rs", "lib/domain.rs"]);
  });

  it("brings recently opened files forward, and shows them first for an empty query", () => {
    const candidates = files("a/x.md", "b/x.md");
    expect(rankFiles("x", candidates, ["p\0b/x.md"])[0].rel).toBe("b/x.md");
    expect(rankFiles("", candidates, ["p\0b/x.md"])[0].rel).toBe("b/x.md");
  });

  it("keeps to the limit", () => {
    expect(rankFiles("", files("a", "b", "c"), [], 2)).toHaveLength(2);
  });

  it("lists the same relative path from two roots separately", () => {
    const candidates = [
      { root: "a", rel: "src/main.rs" },
      { root: "b", rel: "src/main.rs" },
    ];
    const ranked = rankFiles("main", candidates, []);
    expect(ranked).toHaveLength(2);
    expect(ranked.map((r) => r.root).sort()).toEqual(["a", "b"]);
    expect(new Set(ranked.map((r) => `${r.root}\0${r.rel}`)).size).toBe(2);
  });

  it("never lets a recent-file boost put a clearly worse match first", () => {
    const candidates = [
      { root: "a", rel: "y.md" },
      { root: "b", rel: "xyz.md" },
    ];
    // "xyz.md" is the recently opened file, but "y.md" is still the far better match for "y".
    const ranked = rankFiles("y", candidates, ["b\0xyz.md"]);
    expect(ranked[0].rel).toBe("y.md");
  });
});

describe("isFresh", () => {
  it("serves an index for 30 s", () => {
    expect(isFresh({ files: [], truncated: false, at: 1000 }, 20_000)).toBe(true);
    expect(isFresh({ files: [], truncated: false, at: 1000 }, 40_000)).toBe(false);
    expect(isFresh(undefined, 0)).toBe(false);
  });
});
