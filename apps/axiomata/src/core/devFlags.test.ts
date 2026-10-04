import { afterEach, describe, expect, it, vi } from "vitest";

import { autoRefreshDisabled, runUnlessAutoRefreshDisabled } from "./devFlags";

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("autoRefreshDisabled", () => {
  it("is false when the env var is unset", () => {
    expect(autoRefreshDisabled()).toBe(false);
  });

  it("is true only for the exact string \"true\" (vitest's own DEV is true)", () => {
    vi.stubEnv("VITE_AXIOMATA_DISABLE_AUTO_REFRESH", "true");
    expect(autoRefreshDisabled()).toBe(true);
  });

  it("is false for any other value (typo-safe, not just falsy-safe)", () => {
    vi.stubEnv("VITE_AXIOMATA_DISABLE_AUTO_REFRESH", "1");
    expect(autoRefreshDisabled()).toBe(false);
    vi.stubEnv("VITE_AXIOMATA_DISABLE_AUTO_REFRESH", "TRUE");
    expect(autoRefreshDisabled()).toBe(false);
    vi.stubEnv("VITE_AXIOMATA_DISABLE_AUTO_REFRESH", "false");
    expect(autoRefreshDisabled()).toBe(false);
  });

  it("is false outside dev mode even if the var is \"true\" — never active in a production build", () => {
    vi.stubEnv("DEV", false);
    vi.stubEnv("VITE_AXIOMATA_DISABLE_AUTO_REFRESH", "true");
    expect(autoRefreshDisabled()).toBe(false);
  });
});

describe("runUnlessAutoRefreshDisabled", () => {
  it("calls fn when the flag is off", () => {
    const fn = vi.fn();
    runUnlessAutoRefreshDisabled(fn);
    expect(fn).toHaveBeenCalledOnce();
  });

  it("never calls fn when the flag is on — not just discards its result", () => {
    vi.stubEnv("VITE_AXIOMATA_DISABLE_AUTO_REFRESH", "true");
    const fn = vi.fn();
    runUnlessAutoRefreshDisabled(fn);
    expect(fn).not.toHaveBeenCalled();
  });
});
