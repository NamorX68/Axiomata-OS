import { describe, expect, it } from "vitest";

import {
  addDays,
  isoDate,
  monthDiff,
  monthGrid,
  monthOf,
  parseIso,
  shiftMonth,
  todayIso,
  weekRange,
  weekdayLabels,
} from "./monthGrid";

// Local noon on a Tuesday in September 2026.
const NOW = new Date(2026, 8, 15, 12, 0, 0).getTime();
// The same day as a `YYYY-MM-DD` string, for `monthGrid`'s `today` param.
const TODAY = "2026-09-15";

describe("isoDate / parseIso / todayIso", () => {
  it("round-trips a local date without a UTC shift", () => {
    expect(isoDate(new Date(2026, 0, 5))).toBe("2026-01-05");
    expect(isoDate(parseIso("2026-12-31"))).toBe("2026-12-31");
    expect(todayIso(NOW)).toBe("2026-09-15");
  });
});

describe("addDays", () => {
  it("crosses month and year boundaries, both directions", () => {
    expect(addDays("2026-01-31", 1)).toBe("2026-02-01");
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
    expect(addDays("2026-09-15", 7)).toBe("2026-09-22");
  });

  it("is DST-safe (Europe/Berlin spring-forward is the last Sunday of March)", () => {
    // Component arithmetic, not +n*86_400_000 — the calendar date must not
    // slip by the missing hour.
    expect(addDays("2026-03-28", 2)).toBe("2026-03-30");
  });
});

describe("monthOf / shiftMonth / monthDiff", () => {
  it("reads and shifts a displayed month, rolling the year", () => {
    expect(monthOf("2026-09-15")).toEqual({ year: 2026, month: 9 });
    expect(shiftMonth({ year: 2026, month: 1 }, -1)).toEqual({ year: 2025, month: 12 });
    expect(shiftMonth({ year: 2026, month: 12 }, 1)).toEqual({ year: 2027, month: 1 });
    expect(shiftMonth({ year: 2026, month: 6 }, -14)).toEqual({ year: 2025, month: 4 });
  });

  it("monthDiff is a signed month distance across the year boundary", () => {
    expect(monthDiff({ year: 2026, month: 9 }, { year: 2026, month: 9 })).toBe(0);
    expect(monthDiff({ year: 2026, month: 9 }, { year: 2026, month: 11 })).toBe(2);
    expect(monthDiff({ year: 2026, month: 12 }, { year: 2027, month: 1 })).toBe(1);
    expect(monthDiff({ year: 2027, month: 1 }, { year: 2026, month: 12 })).toBe(-1);
  });
});

describe("weekRange", () => {
  it("returns len consecutive days from the start", () => {
    expect(weekRange("2026-09-28")).toEqual([
      "2026-09-28",
      "2026-09-29",
      "2026-09-30",
      "2026-10-01",
      "2026-10-02",
      "2026-10-03",
      "2026-10-04",
    ]);
    expect(weekRange("2026-09-15", 1)).toEqual(["2026-09-15"]);
  });
});

describe("monthGrid", () => {
  it("is always 6×7, Monday-first, starting on the Monday on/before the 1st", () => {
    const g = monthGrid(2026, 9, TODAY); // Sep 1 2026 is a Tuesday
    expect(g.weeks).toHaveLength(6);
    for (const w of g.weeks) expect(w).toHaveLength(7);
    expect(g.weeks[0][0].iso).toBe("2026-08-31"); // the Monday before Sep 1
    expect(g.weeks[0][1]).toMatchObject({ iso: "2026-09-01", inMonth: true });
    // Last cell is 6*7 - 1 = 41 days after the first.
    expect(g.weeks[5][6].iso).toBe(addDays("2026-08-31", 41));
  });

  it("flags exactly one today, only when it falls in the grid", () => {
    const sept = monthGrid(2026, 9, TODAY);
    const flagged = sept.weeks.flat().filter((c) => c.isToday);
    expect(flagged).toHaveLength(1);
    expect(flagged[0].iso).toBe("2026-09-15");

    const march = monthGrid(2026, 3, TODAY);
    expect(march.weeks.flat().some((c) => c.isToday)).toBe(false);
  });

  it("marks adjacent-month filler days inMonth: false", () => {
    const g = monthGrid(2026, 9, TODAY);
    const aug31 = g.weeks[0][0];
    expect(aug31).toMatchObject({ iso: "2026-08-31", inMonth: false });
    // Sep 2026 has 30 days; Wed Sep 30 is in week index 4, then October fills.
    const oct1 = g.weeks.flat().find((c) => c.iso === "2026-10-01");
    expect(oct1?.inMonth).toBe(false);
  });

  it("carries a localised month-year label", () => {
    expect(monthGrid(2026, 9, TODAY).label).toMatch(/2026/);
  });
});

describe("weekdayLabels", () => {
  it("returns 7 distinct labels, Monday-first", () => {
    const short = weekdayLabels("short");
    expect(short).toHaveLength(7);
    expect(new Set(short).size).toBe(7);
    // 2024-01-01 is a Monday, so index 0 is a Monday name.
    expect(short[0]).toBe(new Intl.DateTimeFormat(undefined, { weekday: "short" }).format(new Date(2024, 0, 1)));
    expect(weekdayLabels("narrow")).toHaveLength(7);
  });
});
