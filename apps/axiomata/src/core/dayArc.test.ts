import { describe, expect, it } from "vitest";

import type { CalendarEvent } from "./calendar";
import { arcFraction, arcPath, arcPoint, arcSegments } from "./dayArc";

const at = (h: number, m = 0): number => new Date(2026, 9, 11, h, m).getTime();
const iso = (ms: number): string => new Date(ms).toISOString();

function event(over: Partial<CalendarEvent>): CalendarEvent {
  return {
    id: "e1",
    title: "Team-Sync",
    start: iso(at(9)),
    end: iso(at(10)),
    calendar: "Work",
    location: null,
    allDay: false,
    ...over,
  };
}

describe("arcFraction", () => {
  it("runs from six in the morning to ten at night, clamped", () => {
    expect(arcFraction(at(6))).toBe(0);
    expect(arcFraction(at(14))).toBeCloseTo(0.5);
    expect(arcFraction(at(23))).toBe(1);
    expect(arcFraction(at(3))).toBe(0);
  });
});

describe("arcPoint", () => {
  it("puts the start on the left, noon-ish on top and the end on the right", () => {
    expect(arcPoint(0).x).toBeCloseTo(6);
    expect(arcPoint(0.5)).toEqual({ x: expect.closeTo(50), y: expect.closeTo(8) });
    expect(arcPoint(1).x).toBeCloseTo(94);
  });
});

describe("arcPath", () => {
  it("draws a clockwise arc between two points", () => {
    expect(arcPath(0, 1)).toBe("M 6.00 52.00 A 44 44 0 0 1 94.00 52.00");
  });
});

describe("arcSegments", () => {
  it("places today's timed events and marks the one going on", () => {
    const events = [event({}), event({ id: "e2", start: iso(at(14)), end: iso(at(15)) })];
    const [sync, oneOnOne] = arcSegments(events, at(14, 30));
    expect(sync).toMatchObject({ id: "e1", from: expect.closeTo(0.1875), live: false });
    expect(oneOnOne?.live).toBe(true);
  });

  it("leaves out all-day events, other days and events wholly outside the arc", () => {
    const segments = arcSegments(
      [
        event({ allDay: true, start: "2026-10-11", end: "2026-10-12" }),
        event({ start: iso(at(9) + 86_400_000), end: iso(at(10) + 86_400_000) }),
        event({ start: iso(at(23)), end: iso(at(23, 30)) }),
      ],
      at(12),
    );
    expect(segments).toEqual([]);
  });
});
