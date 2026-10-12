import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentState } from "./backend";
import { startSoundWatch } from "./soundWatch";
import type { Cue, Watched } from "./sounds";

function scene(agents: [number, AgentState][]): Watched {
  return { agents: new Map(agents), routines: new Map() };
}

/** Lets the pending look's promise settle. */
async function settle(): Promise<void> {
  await vi.advanceTimersByTimeAsync(0);
}

describe("startSoundWatch", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("plays the most important cue of a change, only in the background", async () => {
    const looks = [
      scene([[1, "working"], [2, "idle"]]),
      scene([[1, "idle"], [2, "waiting"]]),
      scene([[1, "working"], [2, "waiting"]]),
      scene([[1, "idle"], [2, "waiting"]]),
    ];
    let background = true;
    const played: Cue[] = [];
    const stop = startSoundWatch({
      look: () => Promise.resolve(looks.shift() ?? scene([])),
      play: (cue) => played.push(cue),
      inBackground: () => background,
      intervalMs: 100,
    });
    await settle();
    expect(played).toEqual([]);
    await vi.advanceTimersByTimeAsync(100);
    expect(played).toEqual(["waiting"]);
    await vi.advanceTimersByTimeAsync(100);
    background = false;
    await vi.advanceTimersByTimeAsync(100);
    expect(played).toEqual(["waiting"]);
    stop();
  });

  it("keeps the last scene through a failed look and stops looking when stopped", async () => {
    const look = vi
      .fn<() => Promise<Watched>>()
      .mockResolvedValueOnce(scene([[1, "working"]]))
      .mockRejectedValueOnce(new Error("backend away"))
      .mockResolvedValueOnce(scene([[1, "idle"]]));
    const played: Cue[] = [];
    const stop = startSoundWatch({ look, play: (cue) => played.push(cue), inBackground: () => true, intervalMs: 100 });
    await settle();
    await vi.advanceTimersByTimeAsync(200);
    expect(played).toEqual(["done"]);
    stop();
    await vi.advanceTimersByTimeAsync(500);
    expect(look).toHaveBeenCalledTimes(3);
  });

  it("skips rounds while a look is pending, and gives up on one that never answers", async () => {
    const look = vi
      .fn<() => Promise<Watched>>()
      .mockReturnValueOnce(new Promise<Watched>(() => {}))
      .mockResolvedValue(scene([[1, "waiting"]]));
    const stop = startSoundWatch({
      look,
      play: () => {},
      inBackground: () => true,
      intervalMs: 100,
      timeoutMs: 350,
    });
    await vi.advanceTimersByTimeAsync(300);
    expect(look).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(100);
    expect(look).toHaveBeenCalledTimes(2);
    stop();
  });

  it("plays nothing for a look that comes back after it was stopped", async () => {
    let answer: (w: Watched) => void = () => {};
    const look = vi
      .fn<() => Promise<Watched>>()
      .mockResolvedValueOnce(scene([[1, "working"]]))
      .mockReturnValueOnce(new Promise<Watched>((resolve) => (answer = resolve)));
    const played: Cue[] = [];
    const stop = startSoundWatch({ look, play: (cue) => played.push(cue), inBackground: () => true, intervalMs: 100 });
    await vi.advanceTimersByTimeAsync(100);
    stop();
    answer(scene([[1, "waiting"]]));
    await settle();
    expect(played).toEqual([]);
  });

  it("keeps listening when playing a cue throws", async () => {
    const looks = [scene([[1, "working"]]), scene([[1, "waiting"]]), scene([[1, "working"]]), scene([[1, "idle"]])];
    const tried: Cue[] = [];
    const stop = startSoundWatch({
      look: () => Promise.resolve(looks.shift() ?? scene([[1, "idle"]])),
      play: (cue) => {
        tried.push(cue);
        throw new Error("no audio");
      },
      inBackground: () => true,
      intervalMs: 100,
    });
    await vi.advanceTimersByTimeAsync(300);
    expect(tried).toEqual(["waiting", "done"]);
    stop();
  });
});
