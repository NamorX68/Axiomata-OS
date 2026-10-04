import { describe, expect, it } from "vitest";

import { ViShared } from "../editor/vi/machine";
import { MAX_PERSISTED_REGISTER, restoreVi, snapshotVi } from "./viPersist";

describe("snapshotVi / restoreVi (V4)", () => {
  it("round-trips named registers, file marks, histories and the last search", () => {
    const from = new ViShared(null);
    from.registers.set("a", { text: "one\n", kind: "line" });
    from.registers.set("0", { text: "not kept", kind: "char" });
    from.fileMarks.set("A", { file: "workspace\0notes.md", at: { line: 3, col: 2 } });
    from.search.remember("cmd", "s/a/b/");
    from.search.remember("search", "foo");
    from.search.last = { pattern: "foo", backward: true };

    const to = new ViShared(null);
    restoreVi(to, JSON.parse(JSON.stringify(snapshotVi(from))));
    expect(to.registers.named()).toEqual({ a: { text: "one\n", kind: "line" } });
    expect(to.fileMarks.get("A")).toEqual({ file: "workspace\0notes.md", at: { line: 3, col: 2 } });
    expect(to.search.history).toEqual({ cmd: ["s/a/b/"], search: ["foo"] });
    expect(to.search.last).toEqual({ pattern: "foo", backward: true });
    // Nothing lights up just from starting: hlsearch waits for `n`.
    expect(to.search.highlight).toBe(false);
  });

  it("leaves out a register too large for the file", () => {
    const shared = new ViShared(null);
    shared.registers.set("b", { text: "x".repeat(MAX_PERSISTED_REGISTER + 1), kind: "char" });
    expect(snapshotVi(shared).registers).toEqual({});
  });

  it("skips whatever is malformed and keeps the rest", () => {
    const shared = new ViShared(null);
    restoreVi(shared, {
      version: 1,
      registers: { a: { text: "ok", kind: "char" }, b: { text: 1, kind: "char" }, "0": { text: "x", kind: "char" } },
      fileMarks: {
        A: { file: "f", line: -1, col: 0 },
        B: { file: "g", line: 1, col: 0 },
        a: { file: "h", line: 0, col: 0 },
      },
      history: { cmd: ["w", 3], search: "nope" },
      lastSearch: { pattern: "", backward: false },
    });
    expect(shared.registers.named()).toEqual({ a: { text: "ok", kind: "char" } });
    expect([...shared.fileMarks.keys()]).toEqual(["B"]);
    expect(shared.search.history).toEqual({ cmd: ["w"], search: [] });
    expect(shared.search.last).toBeNull();
  });

  it("ignores a file that is not an object at all", () => {
    const shared = new ViShared(null);
    restoreVi(shared, null);
    restoreVi(shared, [1, 2]);
    expect(shared.registers.named()).toEqual({});
  });

  it("trims a restored history longer than the 100-entry cap to the newest 100, oldest first", () => {
    const shared = new ViShared(null);
    const cmd = Array.from({ length: 150 }, (_, i) => `cmd${i}`);
    restoreVi(shared, { version: 1, history: { cmd, search: [] } });
    expect(shared.search.history.cmd).toHaveLength(100);
    expect(shared.search.history.cmd[0]).toBe("cmd50");
    expect(shared.search.history.cmd[99]).toBe("cmd149");
  });

  it("keeps only the newest 100 entries of a longer in-memory history when snapshotting", () => {
    const shared = new ViShared(null);
    // Bypasses `remember`'s own cap, to exercise snapshotVi's own slice directly.
    for (let i = 0; i < 150; i++) shared.search.history.search.push(`s${i}`);
    const snap = snapshotVi(shared);
    expect(snap.history.search).toHaveLength(100);
    expect(snap.history.search[0]).toBe("s50");
    expect(snap.history.search[99]).toBe("s149");
  });

  it("merges into an already-populated history, moving a duplicate to the end rather than keeping it twice", () => {
    const shared = new ViShared(null);
    shared.search.remember("cmd", "a");
    shared.search.remember("cmd", "b");
    restoreVi(shared, { version: 1, history: { cmd: ["a"], search: [] } });
    // "a" already existed: it moves to the end instead of duplicating.
    expect(shared.search.history.cmd).toEqual(["b", "a"]);
  });

  it("ignores a lastSearch whose backward is not a boolean", () => {
    const shared = new ViShared(null);
    restoreVi(shared, { version: 1, lastSearch: { pattern: "x", backward: "yes" } });
    expect(shared.search.last).toBeNull();
  });
});
