import { describe, expect, it } from "vitest";

import type { FileMatches, SearchSummary } from "./backend";
import { DEFAULT_OPTIONS, parseGlobs, ProjectSearchModel, statusText, toQuery, type SearchPort } from "./projectSearch";

const SUMMARY: SearchSummary = { files: 1, matches: 1, searched: 3, truncated: false, cancelled: false };

function file(rel: string, n = 1): FileMatches {
  return { rel, matches: Array.from({ length: n }, (_, i) => ({ line: i, col: 0, end: 1, text: "x", from: 0 })) };
}

/** A backend whose searches finish when the test says so. */
function controlled() {
  const runs: {
    pattern: string;
    onFiles: (f: FileMatches[]) => void;
    finish: (s: SearchSummary) => void;
    fail: (e: unknown) => void;
  }[] = [];
  const cancelled: string[] = [];
  const port: SearchPort = {
    search: (_root, query, _owner, onFiles) =>
      new Promise((finish, fail) => runs.push({ pattern: query.pattern, onFiles, finish, fail })),
    cancel: async (owner) => {
      cancelled.push(owner);
    },
  };
  return { port, runs, cancelled };
}

describe("parseGlobs and toQuery", () => {
  it("splits comma-separated globs and drops empty ones", () => {
    expect(parseGlobs(" *.rs, src/** ,, ")).toEqual(["*.rs", "src/**"]);
    expect(toQuery("x", { ...DEFAULT_OPTIONS, regex: true, include: "*.md" })).toEqual({
      pattern: "x",
      regex: true,
      caseSensitive: false,
      wholeWord: false,
      include: ["*.md"],
      exclude: [],
    });
  });
});

describe("ProjectSearchModel (T14)", () => {
  it("collects the files as they arrive and ends with the summary", async () => {
    const { port, runs } = controlled();
    const m = new ProjectSearchModel(port, "me", () => {});
    const done = m.start("workspace", "x", DEFAULT_OPTIONS);
    expect(m.view.status).toBe("running");
    runs[0].onFiles([file("a.md", 2)]);
    runs[0].onFiles([file("b.md")]);
    expect(m.view.files.map((f) => f.rel)).toEqual(["a.md", "b.md"]);
    expect(m.view.matches).toBe(3);
    runs[0].finish({ ...SUMMARY, truncated: true });
    await done;
    expect(m.view).toMatchObject({ status: "done", truncated: true, matches: 3 });
  });

  it("drops what an older search still sends once a newer one started", async () => {
    const { port, runs } = controlled();
    const m = new ProjectSearchModel(port, "me", () => {});
    const first = m.start("workspace", "x", DEFAULT_OPTIONS);
    const second = m.start("workspace", "xy", DEFAULT_OPTIONS);
    runs[0].onFiles([file("old.md")]);
    runs[0].finish(SUMMARY);
    await first;
    expect(m.view.status).toBe("running");
    runs[1].onFiles([file("new.md")]);
    runs[1].finish(SUMMARY);
    await second;
    expect(m.view.files.map((f) => f.rel)).toEqual(["new.md"]);
  });

  it("an empty query clears the results and stops the running search", async () => {
    const { port, runs, cancelled } = controlled();
    const m = new ProjectSearchModel(port, "me", () => {});
    void m.start("workspace", "x", DEFAULT_OPTIONS);
    runs[0].onFiles([file("a.md")]);
    await m.start("workspace", "", DEFAULT_OPTIONS);
    expect(m.view).toMatchObject({ status: "idle", files: [] });
    expect(cancelled).toEqual(["me"]);
  });

  it("names a bad glob as a glob, not as the pattern", async () => {
    const { port, runs } = controlled();
    const m = new ProjectSearchModel(port, "me", () => {});
    const done = m.start("workspace", "x", { ...DEFAULT_OPTIONS, include: "{" });
    runs[0].fail({ kind: "BadPattern", message: "bad search pattern: include/exclude glob `{`: unclosed" });
    await done;
    expect(m.view.error).toBe("Invalid include/exclude glob `{`: unclosed");
  });

  it("says what is wrong with a bad pattern", async () => {
    const { port, runs } = controlled();
    const m = new ProjectSearchModel(port, "me", () => {});
    const done = m.start("workspace", "(", { ...DEFAULT_OPTIONS, regex: true });
    runs[0].fail({ kind: "BadPattern", message: "bad search pattern: unclosed group" });
    await done;
    expect(m.view).toMatchObject({ status: "error", error: "Invalid pattern: unclosed group" });
  });
});

describe("statusText", () => {
  const view = { status: "done" as const, files: [file("a"), file("b")], matches: 1234, truncated: false, error: null };

  it("counts results and files, and says when it stopped early", () => {
    expect(statusText(view)).toBe(`${(1234).toLocaleString()} results in 2 files`);
    expect(statusText({ ...view, truncated: true })).toMatch(/stopped at 10,000 results/);
    expect(statusText({ ...view, status: "running", matches: 0, files: [] })).toBe("Searching…");
    expect(statusText({ ...view, matches: 0, files: [] })).toBe("No results");
  });
});
