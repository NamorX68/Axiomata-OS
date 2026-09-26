import { afterEach, describe, expect, it, vi } from "vitest";

import { invokeBackend } from "../core/backend";
import { createFile } from "./backend";

vi.mock("../core/backend", () => ({ invokeBackend: vi.fn() }));

const invokeMock = vi.mocked(invokeBackend);

describe("createFile (W13)", () => {
  afterEach(() => {
    invokeMock.mockReset();
  });

  it("asks Rust for an exclusive create — one call, no read-then-write race", async () => {
    invokeMock.mockResolvedValueOnce("v1");
    await expect(createFile("workspace", "a.md")).resolves.toBe("v1");
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("file_create", { root: "workspace", rel: "a.md" });
  });

  it("passes on the refusal when something is already there", async () => {
    invokeMock.mockRejectedValueOnce({ kind: "Refused", message: "a.md: something with this name is already there" });
    await expect(createFile("workspace", "a.md")).rejects.toMatchObject({ kind: "Refused" });
  });
});
