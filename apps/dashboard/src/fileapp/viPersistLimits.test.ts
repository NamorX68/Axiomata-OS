/** The size limits of `editor-vi.json` (ED3.4 security review): the file must never outgrow what loads back. */
import { describe, expect, it } from "vitest";

import { ViShared } from "../editor/vi/machine";
import { MAX_PERSISTED_REGISTER, MAX_PERSISTED_REGISTERS_TOTAL, restoreVi, snapshotVi } from "./viPersist";

describe("editor-vi.json size limits", () => {
  it("writes registers only up to the total budget, in name order", () => {
    const shared = new ViShared(null);
    const big = "x".repeat(MAX_PERSISTED_REGISTER);
    for (const name of "abcdefghijklmnopqrstuvwxyz") shared.registers.set(name, { text: big, kind: "char" });
    const kept = Object.keys(snapshotVi(shared).registers);
    expect(kept.length * MAX_PERSISTED_REGISTER).toBeLessThanOrEqual(MAX_PERSISTED_REGISTERS_TOTAL);
    expect(kept[0]).toBe("a");
    expect(JSON.stringify(snapshotVi(shared)).length).toBeLessThan(4 * 1024 * 1024);
  });

  it("does not read back a register the app would not have written", () => {
    const shared = new ViShared(null);
    restoreVi(shared, { version: 1, registers: { a: { text: "x".repeat(MAX_PERSISTED_REGISTER + 1), kind: "char" } } });
    expect(shared.registers.named()).toEqual({});
  });

  it("leaves out an overlong history entry both ways", () => {
    const shared = new ViShared(null);
    const long = "s".repeat(5000);
    shared.search.remember("cmd", long);
    shared.search.remember("cmd", "w");
    expect(snapshotVi(shared).history.cmd).toEqual(["w"]);
    const other = new ViShared(null);
    restoreVi(other, { version: 1, history: { cmd: [long, "q"], search: [] } });
    expect(other.search.history.cmd).toEqual(["q"]);
  });
});
