import { describe, expect, it } from "vitest";

import type { AgentState } from "./backend";
import { CUE_ORDER, CUE_VOICES, cuesBetween, sceneOf, type Watched } from "./sounds";

function scene(agents: [number, AgentState][], routines: [number, string | null][] = []): Watched {
  return { agents: new Map(agents), routines: new Map(routines) };
}

describe("cuesBetween", () => {
  it("only sets the scene on the first look", () => {
    expect(cuesBetween(null, scene([[1, "waiting"]], [[1, "2026-10-12T10:00:00Z"]]))).toEqual([]);
  });

  it("calls for waiting when an agent starts to wait, also a new one, but not while it keeps waiting", () => {
    expect(cuesBetween(scene([[1, "working"]]), scene([[1, "waiting"]]))).toEqual(["waiting"]);
    expect(cuesBetween(scene([]), scene([[2, "waiting"]]))).toEqual(["waiting"]);
    expect(cuesBetween(scene([[1, "waiting"]]), scene([[1, "waiting"]]))).toEqual([]);
  });

  it("calls for done when a working agent goes idle or ends, and for nothing else", () => {
    expect(cuesBetween(scene([[1, "working"]]), scene([[1, "idle"]]))).toEqual(["done"]);
    expect(cuesBetween(scene([[1, "working"]]), scene([[1, "ended"]]))).toEqual(["done"]);
    expect(cuesBetween(scene([[1, "starting"]]), scene([[1, "idle"]]))).toEqual([]);
    expect(cuesBetween(scene([[1, "waiting"]]), scene([[1, "working"]]))).toEqual([]);
    expect(cuesBetween(scene([[1, "working"]]), scene([]))).toEqual([]);
  });

  it("calls for routine when a known routine's last firing changed, not for a new or never-fired one", () => {
    const before = scene([], [[1, "2026-10-12T10:00:00Z"], [2, null]]);
    expect(cuesBetween(before, scene([], [[1, "2026-10-12T10:05:00Z"], [2, null]]))).toEqual(["routine"]);
    expect(cuesBetween(before, scene([], [[1, "2026-10-12T10:00:00Z"], [2, "2026-10-12T10:05:00Z"]]))).toEqual([
      "routine",
    ]);
    expect(cuesBetween(before, scene([], [[1, "2026-10-12T10:00:00Z"], [3, "2026-10-12T10:05:00Z"]]))).toEqual([]);
    expect(cuesBetween(before, before)).toEqual([]);
  });

  it("lists every cue once, the most important first", () => {
    const prev = scene(
      [
        [1, "working"],
        [2, "working"],
        [3, "idle"],
      ],
      [[1, null]],
    );
    const next = scene(
      [
        [1, "idle"],
        [2, "ended"],
        [3, "waiting"],
      ],
      [[1, "2026-10-12T10:05:00Z"]],
    );
    expect(cuesBetween(prev, next)).toEqual(["waiting", "done", "routine"]);
  });
});

describe("sceneOf", () => {
  it("puts every project's agents into one map and keeps each routine's last firing", () => {
    const scene = sceneOf(
      [
        [{ agent_id: 1, state: "working" }],
        [
          { agent_id: 4, state: "waiting" },
          { agent_id: 5, state: "idle" },
        ],
      ],
      [
        { id: 2, last_fired_at: null },
        { id: 3, last_fired_at: "2026-10-12T10:00:00Z" },
      ],
    );
    expect([...scene.agents]).toEqual([
      [1, "working"],
      [4, "waiting"],
      [5, "idle"],
    ]);
    expect([...scene.routines]).toEqual([
      [2, null],
      [3, "2026-10-12T10:00:00Z"],
    ]);
  });
});

describe("the voices", () => {
  it("are short and soft for every cue", () => {
    for (const cue of CUE_ORDER) {
      expect(Math.max(...CUE_VOICES[cue].map((v) => v.at + v.dur))).toBeLessThanOrEqual(1.2);
      for (const voice of CUE_VOICES[cue]) {
        expect(voice.gain).toBeGreaterThan(0);
        expect(voice.gain).toBeLessThanOrEqual(1);
        expect(voice.dur).toBeGreaterThan(0);
      }
    }
  });
});
