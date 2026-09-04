import { describe, expect, it } from "vitest";

import { eventTimeLabel, filterByCalendar, groupByDay, parseCalendarDigest, type CalendarEvent } from "./calendar";

const DIGEST_JSON = JSON.stringify({
  calendars: ["Arbeit", "Privat", "Familie"],
  events: [
    { title: "Team-Sync", start: "2026-09-05T09:00:00", end: "2026-09-05T09:30:00", calendar: "Arbeit", location: null, allDay: false },
    { title: "Zahnarzt", start: "2026-09-06", end: "2026-09-06", calendar: "Privat", location: "Praxis Beispiel", allDay: true },
    { title: "Kino", start: "2026-09-06T19:00:00", end: "2026-09-06T21:30:00", calendar: "Familie", location: "Cineplex", allDay: false },
  ],
});

describe("parseCalendarDigest", () => {
  it("parses a well-formed digest", () => {
    const d = parseCalendarDigest(DIGEST_JSON);
    expect(d.calendars).toEqual(["Arbeit", "Privat", "Familie"]);
    expect(d.events).toHaveLength(3);
    expect(d.events[0].title).toBe("Team-Sync");
  });

  it("strips a ```json fence the model added despite being told not to", () => {
    const fenced = "```json\n" + DIGEST_JSON + "\n```";
    expect(parseCalendarDigest(fenced)).toEqual(parseCalendarDigest(DIGEST_JSON));
  });

  it("strips a bare ``` fence too", () => {
    const fenced = "```\n" + DIGEST_JSON + "\n```";
    expect(parseCalendarDigest(fenced)).toEqual(parseCalendarDigest(DIGEST_JSON));
  });

  it("throws the skill's own error message when it reports no calendar tool", () => {
    const noTool = JSON.stringify({ calendars: [], events: [], error: "no calendar tool available" });
    expect(() => parseCalendarDigest(noTool)).toThrow("no calendar tool available");
  });

  it("throws on unparseable output", () => {
    expect(() => parseCalendarDigest("not json at all")).toThrow(/not valid JSON/);
  });

  it("throws when the JSON isn't an object", () => {
    expect(() => parseCalendarDigest("42")).toThrow(/not a JSON object/);
  });

  it("drops malformed entries instead of throwing on the whole digest", () => {
    const mixed = JSON.stringify({
      calendars: ["Arbeit", 42, null],
      events: [
        { title: "ok", start: "2026-09-05", end: "2026-09-05", calendar: "Arbeit", location: null, allDay: true },
        { title: "missing calendar field" },
        "not even an object",
      ],
    });
    const d = parseCalendarDigest(mixed);
    expect(d.calendars).toEqual(["Arbeit"]);
    expect(d.events).toHaveLength(1);
    expect(d.events[0].title).toBe("ok");
  });
});

describe("filterByCalendar", () => {
  it("returns everything when the filter is null (\"all calendars\")", () => {
    const { events } = parseCalendarDigest(DIGEST_JSON);
    expect(filterByCalendar(events, null)).toHaveLength(3);
  });

  it("keeps only events on the named calendar", () => {
    const { events } = parseCalendarDigest(DIGEST_JSON);
    const familie = filterByCalendar(events, "Familie");
    expect(familie.map((e) => e.title)).toEqual(["Kino"]);
  });

  it("returns an empty list for a calendar with no upcoming events", () => {
    const { events } = parseCalendarDigest(DIGEST_JSON);
    expect(filterByCalendar(events, "Müll")).toEqual([]);
  });
});

describe("groupByDay", () => {
  it("groups consecutive same-day events, in encounter order", () => {
    const { events } = parseCalendarDigest(DIGEST_JSON);
    const groups = groupByDay(events);
    expect(groups.map((g) => g.day)).toEqual(["2026-09-05", "2026-09-06"]);
    expect(groups[1].events.map((e) => e.title)).toEqual(["Zahnarzt", "Kino"]);
  });

  it("returns no groups for no events", () => {
    expect(groupByDay([])).toEqual([]);
  });
});

describe("eventTimeLabel", () => {
  const base: CalendarEvent = { title: "x", start: "2026-09-05T09:00:00", end: "2026-09-05T09:30:00", calendar: "c", location: null, allDay: false };

  it("shows a start–end range for a timed event", () => {
    expect(eventTimeLabel(base)).toBe("09:00–09:30");
  });

  it("shows \"All day\" for an all-day event regardless of the date-only start/end", () => {
    expect(eventTimeLabel({ ...base, start: "2026-09-05", end: "2026-09-05", allDay: true })).toBe("All day");
  });
});
