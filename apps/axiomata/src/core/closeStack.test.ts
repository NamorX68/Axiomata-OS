import { describe, expect, it, vi } from "vitest";

import { closeTop, dialogOpen, pushClose } from "./closeStack";

describe("closeStack", () => {
  it("closes nothing when no window is open", () => {
    expect(closeTop()).toBe(false);
  });

  it("closes the newest window first", () => {
    const older = vi.fn();
    const newer = vi.fn();
    const offOlder = pushClose(older);
    const offNewer = pushClose(newer);
    expect(closeTop()).toBe(true);
    expect(newer).toHaveBeenCalledOnce();
    expect(older).not.toHaveBeenCalled();
    offNewer();
    offOlder();
  });

  it("forgets a window that has gone", () => {
    const gone = vi.fn();
    const off = pushClose(gone);
    expect(dialogOpen()).toBe(true);
    off();
    expect(dialogOpen()).toBe(false);
    expect(closeTop()).toBe(false);
    expect(gone).not.toHaveBeenCalled();
  });
});
