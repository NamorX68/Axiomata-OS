import { describe, expect, it } from "vitest";

import type { RunRecord, RunSummary } from "./backend";
import { eventTimeLabel, filterByCalendar, groupByDay, loadLatestCalendarDigest, parseCalendarDigest, type CalendarEvent } from "./calendar";

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

describe("loadLatestCalendarDigest", () => {
  const summary = (over: Partial<RunSummary>): RunSummary => ({
    id: 1,
    skill_name: "calendar-digest",
    backend: "claude-code",
    status: "success",
    exit_code: 0,
    duration_ms: 100,
    error: null,
    started_at: "2026-09-05T09:00:00Z",
    ...over,
  });

  /** A fake `invoke` serving `list_runs` from `runs` and `get_run` from
   *  `records`, matching the two calls the loader actually makes. */
  function fakeInvoke(runs: RunSummary[], records: Record<number, RunRecord>) {
    return (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (cmd === "list_runs") return runs as unknown as T;
      if (cmd === "get_run") return (records[(args as { id: number }).id] ?? null) as unknown as T;
      throw new Error(`unexpected cmd ${cmd}`);
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  }

  it("returns an empty digest with no error when the skill has never run", async () => {
    const result = await loadLatestCalendarDigest(fakeInvoke([], {}));
    expect(result).toEqual({ run: null, digest: { calendars: [], events: [] }, error: null });
  });

  it("ignores other skills' runs and picks the newest calendar-digest one", async () => {
    const runs = [summary({ id: 2, skill_name: "newsletter" }), summary({ id: 1 })];
    const records = { 1: { ...summary({ id: 1 }), stdout: DIGEST_JSON, stderr: "", finished_at: "" } };
    const result = await loadLatestCalendarDigest(fakeInvoke(runs, records));
    expect(result.run?.id).toBe(1);
    expect(result.digest.events).toHaveLength(3);
  });

  it("surfaces a failed run's own error without fetching the full record", async () => {
    const runs = [summary({ status: "failed", error: "agent timed out" })];
    const result = await loadLatestCalendarDigest(fakeInvoke(runs, {}));
    expect(result.error).toBe("agent timed out");
    expect(result.digest).toEqual({ calendars: [], events: [] });
  });

  it("reports a clear error when the run record is missing", async () => {
    const result = await loadLatestCalendarDigest(fakeInvoke([summary({})], {}));
    expect(result.error).toBe("Run record not found.");
  });

  it("surfaces a parse error from a malformed successful run", async () => {
    const records = { 1: { ...summary({}), stdout: "not json", stderr: "", finished_at: "" } };
    const result = await loadLatestCalendarDigest(fakeInvoke([summary({})], records));
    expect(result.error).toMatch(/not valid JSON/);
  });
});
