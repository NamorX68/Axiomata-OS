import { describe, expect, it } from "vitest";

import { absoluteTime, formatBytes, relativeTime, shortPath, untilTime } from "./format";

const NOW = Date.parse("2026-09-03T12:00:00Z");
const at = (ms: number) => new Date(NOW + ms).toISOString();

describe("relativeTime", () => {
  it("buckets past timestamps", () => {
    expect(relativeTime(null)).toBe("never");
    expect(relativeTime("garbage")).toBe("garbage");
    expect(relativeTime(at(-10_000), NOW)).toBe("just now");
    expect(relativeTime(at(-5 * 60_000), NOW)).toBe("5 min ago");
    expect(relativeTime(at(-3 * 3_600_000), NOW)).toBe("3 h ago");
    expect(relativeTime(at(-2 * 86_400_000), NOW)).toBe("2 d ago");
    expect(relativeTime(at(-30 * 86_400_000), NOW)).toMatch(/\d/);
  });
});

describe("untilTime", () => {
  it("buckets future timestamps", () => {
    expect(untilTime(null)).toBe("—");
    expect(untilTime(at(-1), NOW)).toBe("due");
    expect(untilTime(at(30_000), NOW)).toBe("in <1 min");
    expect(untilTime(at(5 * 60_000), NOW)).toBe("in 5 min");
    expect(untilTime(at(2 * 3_600_000), NOW)).toBe("in 2 h");
    expect(untilTime(at(3 * 86_400_000), NOW)).toBe("in 3 d");
  });
});

describe("shortPath", () => {
  it("keeps the last segments", () => {
    expect(shortPath("/a/b/c/d")).toBe("…/c/d");
    expect(shortPath("/a/b/c/d", 3)).toBe("…/b/c/d");
    expect(shortPath("a/b")).toBe("a/b");
  });
});

describe("formatBytes / absoluteTime", () => {
  it("formats sizes", () => {
    expect(formatBytes(340)).toBe("340 B");
    expect(formatBytes(1300)).toBe("1.3 KB");
    expect(formatBytes(3.4 * 1024 * 1024)).toBe("3.4 MB");
    expect(formatBytes(-1)).toBe("—");
  });
  it("formats absolute times", () => {
    expect(absoluteTime(null)).toBe("—");
    expect(absoluteTime("garbage")).toBe("garbage");
    expect(absoluteTime("2026-09-03T12:00:00Z")).toMatch(/2026/);
  });
});
