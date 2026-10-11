import { describe, expect, it } from "vitest";

import type { BoardCard, RunSummary, SpendSummary } from "./backend";
import { beatsFromCards, beatsFromRuns, dayOf, dayShare, fuel, placeOnDay } from "./heartbeat";

/** Local time on 11 Oct 2026 at `h:m`, in ms. */
const today = (h: number, m = 0): number => new Date(2026, 9, 11, h, m).getTime();
const iso = (ms: number): string => new Date(ms).toISOString();
const NOW = today(15);

function run(over: Partial<RunSummary>): RunSummary {
  return {
    id: 1,
    skill_name: "mail-digest",
    backend: "opencode",
    status: "success",
    exit_code: 0,
    duration_ms: 60_000,
    error: null,
    started_at: iso(today(9)),
    source: "manual",
    ...over,
  };
}

function card(over: Partial<BoardCard>): BoardCard {
  return {
    id: 4,
    board_id: 1,
    title: "Drag and drop",
    claimed_at: iso(today(10)),
    claimed_by: "agent:claude-1",
    verified_at: null,
    failed_at: null,
    canceled_at: null,
    ...over,
  } as BoardCard;
}

function only<T>(items: T[]): T {
  const [item] = items;
  if (item === undefined || items.length !== 1) throw new Error(`expected one item, got ${items.length}`);
  return item;
}

function spend(over: Partial<SpendSummary>): SpendSummary {
  return {
    provider: "open_router",
    role: "chat & skill",
    today_usd: 0,
    month_usd: 0,
    daily_cap_usd: 2,
    metered: true,
    ...over,
  };
}

describe("dayOf", () => {
  it("runs from local midnight to the next", () => {
    expect(dayOf(NOW)).toEqual({ start: today(0), end: new Date(2026, 9, 12).getTime() });
  });
});

describe("beatsFromRuns (G14)", () => {
  it("makes a spark as long as the run, red when it failed", () => {
    const [ok, bad] = beatsFromRuns([run({ id: 1 }), run({ id: 2, status: "failed" })], NOW);
    expect(ok).toMatchObject({ key: "run:1", start: today(9), end: today(9, 1), tone: "ok" });
    expect(bad?.tone).toBe("bad");
  });

  it("leaves out yesterday's runs and names a routine's", () => {
    const beats = beatsFromRuns([run({ started_at: iso(today(0) - 1) }), run({ id: 3, source: "routine" })], NOW);
    expect(beats.map((b) => b.label)).toEqual(["mail-digest (routine)"]);
  });
});

describe("beatsFromCards (G14)", () => {
  it("glows while the card is claimed and not done", () => {
    expect(beatsFromCards([card({})], NOW)[0]).toMatchObject({ kind: "card", tone: "live", end: null, boardId: 1 });
  });

  it("ends at the sign-off or the failure", () => {
    const [done, failed] = beatsFromCards(
      [card({ id: 1, verified_at: iso(today(11)) }), card({ id: 2, failed_at: iso(today(12)) })],
      NOW,
    );
    expect(done).toMatchObject({ tone: "ok", end: today(11) });
    expect(failed).toMatchObject({ tone: "bad", end: today(12) });
  });

  it("starts a session from yesterday at midnight, and skips one that ended yesterday", () => {
    const beats = beatsFromCards(
      [
        card({ id: 1, claimed_at: iso(today(0) - 3_600_000) }),
        card({ id: 2, claimed_at: iso(today(0) - 7_200_000), verified_at: iso(today(0) - 3_600_000) }),
      ],
      NOW,
    );
    expect(beats).toHaveLength(1);
    expect(beats[0]?.start).toBe(today(0));
  });

  it("skips an unclaimed card and one released without an end", () => {
    expect(beatsFromCards([card({ claimed_at: null }), card({ claimed_by: null })], NOW)).toEqual([]);
  });
});

describe("placeOnDay", () => {
  it("puts three in the afternoon five eighths into the day", () => {
    expect(dayShare(NOW, NOW)).toBeCloseTo(62.5);
  });

  it("places a spark by its share of the day, and gives a short one a minimum width", () => {
    const place = placeOnDay(only(beatsFromRuns([run({ started_at: iso(today(12)) })], NOW)), NOW);
    expect(place.left).toBeCloseTo(50);
    expect(place.width).toBeCloseTo(0.6);
  });

  it("draws a running session up to now", () => {
    const place = placeOnDay(only(beatsFromCards([card({ claimed_at: iso(today(12)) })], NOW)), NOW);
    expect(place.width).toBeCloseTo(12.5);
  });
});

describe("fuel (G15)", () => {
  it("fills hex segments against the daily limit and turns hot from 80 %", () => {
    expect(fuel([spend({ today_usd: 1 })])).toMatchObject({ todayUsd: 1, capUsd: 2, filled: 3, hot: false });
    expect(fuel([spend({ today_usd: 1.6 })]).hot).toBe(true);
  });

  it("shows only the amount without a limit, and counts metered providers only", () => {
    const tank = fuel([
      spend({ today_usd: 0.5, daily_cap_usd: null }),
      spend({ today_usd: 9, metered: false, daily_cap_usd: null }),
    ]);
    expect(tank).toEqual({ todayUsd: 0.5, capUsd: null, filled: 0, hot: false });
  });
});
