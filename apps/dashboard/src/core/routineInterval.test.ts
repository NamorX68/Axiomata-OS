import { describe, expect, it } from "vitest";

import { cronToInterval, defaultInterval, describeInterval, intervalToCron, WEEKDAY_NAMES, type RoutineInterval } from "./routineInterval";

describe("intervalToCron / cronToInterval", () => {
  it("round-trips every generated shape", () => {
    const cases: RoutineInterval[] = [
      { kind: "minutes", everyMinutes: 15 },
      { kind: "hourly", everyHours: 2, minute: 30 },
      { kind: "daily", hour: 9, minute: 0 },
      { kind: "weekly", weekday: 1, hour: 8, minute: 45 },
    ];
    for (const interval of cases) {
      const cron = intervalToCron(interval);
      expect(cronToInterval(cron)).toEqual(interval);
    }
  });

  it("compiles the expected literal cron strings", () => {
    expect(intervalToCron({ kind: "minutes", everyMinutes: 5 })).toBe("0 */5 * * * *");
    expect(intervalToCron({ kind: "hourly", everyHours: 3, minute: 0 })).toBe("0 0 */3 * * *");
    expect(intervalToCron({ kind: "daily", hour: 14, minute: 30 })).toBe("0 30 14 * * *");
    expect(intervalToCron({ kind: "weekly", weekday: 0, hour: 7, minute: 0 })).toBe("0 0 7 * * Sun");
    expect(intervalToCron({ kind: "weekly", weekday: 6, hour: 7, minute: 0 })).toBe("0 0 7 * * Sat");
  });

  it("clamps out-of-range numeric fields instead of producing an invalid expression", () => {
    expect(intervalToCron({ kind: "minutes", everyMinutes: 0 })).toBe("0 */1 * * * *");
    expect(intervalToCron({ kind: "minutes", everyMinutes: 999 })).toBe("0 */59 * * * *");
    expect(intervalToCron({ kind: "daily", hour: -1, minute: 90 })).toBe("0 59 0 * * *");
  });

  it("falls back to custom for anything it did not generate", () => {
    const weird = "0 0 3 1 1 * 2030";
    expect(cronToInterval(weird)).toEqual({ kind: "custom", cron: weird });

    const fiveField = "*/2 * * * *"; // valid crontab, but not this app's dialect
    expect(cronToInterval(fiveField)).toEqual({ kind: "custom", cron: fiveField });
  });

  it("passes a custom cron straight through untouched (aside from trimming)", () => {
    const raw = "  0 0 3 * * Mon  ";
    expect(intervalToCron({ kind: "custom", cron: raw })).toBe("0 0 3 * * Mon");
  });

  it("every weekday name round-trips to its own index", () => {
    WEEKDAY_NAMES.forEach((_name, weekday) => {
      const interval: RoutineInterval = { kind: "weekly", weekday, hour: 6, minute: 15 };
      expect(cronToInterval(intervalToCron(interval))).toEqual(interval);
    });
  });
});

describe("defaultInterval", () => {
  it("starts a new routine at a sensible daily default", () => {
    expect(defaultInterval()).toEqual({ kind: "daily", hour: 9, minute: 0 });
  });
});

describe("describeInterval", () => {
  it("summarises each kind for display", () => {
    expect(describeInterval({ kind: "minutes", everyMinutes: 15 })).toBe("every 15 min");
    expect(describeInterval({ kind: "hourly", everyHours: 1, minute: 0 })).toBe("hourly at :00");
    expect(describeInterval({ kind: "hourly", everyHours: 4, minute: 30 })).toBe("every 4h at :30");
    expect(describeInterval({ kind: "daily", hour: 9, minute: 5 })).toBe("daily at 09:05");
    expect(describeInterval({ kind: "weekly", weekday: 1, hour: 8, minute: 0 })).toBe("weekly on Mon at 08:00");
    expect(describeInterval({ kind: "custom", cron: "0 0 3 * * *" })).toBe("0 0 3 * * *");
  });
});
