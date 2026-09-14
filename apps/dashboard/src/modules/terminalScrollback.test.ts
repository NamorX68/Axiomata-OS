import { describe, expect, it } from "vitest";

import { createSequenceGuard } from "./terminalScrollback";

describe("createSequenceGuard", () => {
  it("the first request's number is current until another one starts", () => {
    const guard = createSequenceGuard();
    const first = guard.next();
    expect(guard.isCurrent(first)).toBe(true);
  });

  it("starting a new request makes every earlier one stale", () => {
    const guard = createSequenceGuard();
    const first = guard.next();
    const second = guard.next();
    expect(guard.isCurrent(first)).toBe(false);
    expect(guard.isCurrent(second)).toBe(true);
  });

  it("resolves out of order - a later request's response still wins over an earlier one that resolves after it", () => {
    const guard = createSequenceGuard();
    const first = guard.next();
    const second = guard.next();
    // Simulate `second`'s response landing first, then `first`'s late one.
    expect(guard.isCurrent(second)).toBe(true);
    expect(guard.isCurrent(first)).toBe(false);
  });

  it("never mints the same sequence number twice", () => {
    const guard = createSequenceGuard();
    const seen = new Set<number>();
    for (let i = 0; i < 5; i++) seen.add(guard.next());
    expect(seen.size).toBe(5);
  });
});
