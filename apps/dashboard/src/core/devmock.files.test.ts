import { describe, expect, it } from "vitest";

import type { TextFile } from "./backend";
import { mockInvoke } from "./devmock";

describe("devmock file service", () => {
  it("reads back what it wrote and refuses a stale version", async () => {
    const v1 = await mockInvoke<string>("file_write", { root: "project:1", rel: "a.rs", content: "one", expected: null });
    const read = await mockInvoke<TextFile>("file_read", { root: "project:1", rel: "a.rs" });
    expect(read).toMatchObject({ content: "one", version: v1, large: false });

    const v2 = await mockInvoke<string>("file_write", { root: "project:1", rel: "a.rs", content: "two", expected: v1 });
    expect(v2).not.toBe(v1);
    await expect(
      mockInvoke("file_write", { root: "project:1", rel: "a.rs", content: "three", expected: v1 }),
    ).rejects.toMatchObject({ kind: "Conflict" });
  });

  it("refuses escapes and unknown roots with the Rust error kinds", async () => {
    await expect(mockInvoke("file_read", { root: "workspace", rel: "../x" })).rejects.toMatchObject({
      kind: "Refused",
    });
    await expect(mockInvoke("file_read", { root: "/etc", rel: "hosts" })).rejects.toMatchObject({
      kind: "UnknownRoot",
    });
    await expect(mockInvoke("file_read", { root: "workspace", rel: "missing.md" })).rejects.toMatchObject({
      kind: "NotFound",
    });
  });
});
