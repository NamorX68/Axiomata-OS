import { describe, expect, it } from "vitest";

import { fileHandle, registerFileHandle, type FileHandle } from "./fileHandles";

const handle = (): FileHandle => ({ hasUnsaved: () => true, saveNow: async () => true, discard: async () => {} });

describe("fileHandles", () => {
  it("finds a registered handle and forgets it on unregister", () => {
    const h = handle();
    const off = registerFileHandle("t1", h);
    expect(fileHandle("t1")).toBe(h);
    off();
    expect(fileHandle("t1")).toBeNull();
  });

  it("does not drop a newer handle when an older registration is undone", () => {
    const old = handle();
    const offOld = registerFileHandle("t2", old);
    const next = handle();
    registerFileHandle("t2", next);
    offOld();
    expect(fileHandle("t2")).toBe(next);
  });
});
