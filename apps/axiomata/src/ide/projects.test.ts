import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { invokeBackend } from "../core/backend";
import { toast } from "../core/toast";
import {
  LAYOUT_SAVE_DEBOUNCE_MS,
  cancelLayoutWrite,
  flushLayout,
  resetLayoutWritesForTests,
  saveLayoutSoon,
} from "./projects";

vi.mock("../core/backend", () => ({ invokeBackend: vi.fn(async () => true) }));
vi.mock("../core/toast", () => ({ toast: vi.fn() }));

const invoked = vi.mocked(invokeBackend);
const toasted = vi.mocked(toast);

/** Every `set_ide_project_layout` call so far, as `[id, layout]` pairs. */
function writes(): [number, string | null][] {
  return invoked.mock.calls
    .filter(([cmd]) => cmd === "set_ide_project_layout")
    .map(([, args]) => [(args as { id: number }).id, (args as { layout: string | null }).layout]);
}

beforeEach(() => {
  vi.useFakeTimers();
  resetLayoutWritesForTests();
  invoked.mockReset();
  invoked.mockResolvedValue(true);
  toasted.mockClear();
});

afterEach(async () => {
  vi.useRealTimers();
  await flushLayout();
});

describe("saveLayoutSoon", () => {
  it("writes nothing until the debounce elapses", () => {
    saveLayoutSoon(1, "{a}");
    expect(writes()).toEqual([]);
    vi.advanceTimersByTime(LAYOUT_SAVE_DEBOUNCE_MS);
    expect(writes()).toEqual([[1, "{a}"]]);
  });

  it("collapses a burst into one write of the last layout", () => {
    // What a divider drag looks like: one call per pointer move.
    for (const layout of ["{a}", "{b}", "{c}"]) saveLayoutSoon(1, layout);
    vi.advanceTimersByTime(LAYOUT_SAVE_DEBOUNCE_MS);
    expect(writes()).toEqual([[1, "{c}"]]);
  });

  it("writes the previous project's layout before queueing another's", () => {
    // The case that silently loses work: switching projects while a write is
    // still waiting. The pending one must not simply be replaced.
    saveLayoutSoon(1, "{one}");
    saveLayoutSoon(2, "{two}");
    expect(writes()).toEqual([[1, "{one}"]]);
    vi.advanceTimersByTime(LAYOUT_SAVE_DEBOUNCE_MS);
    expect(writes()).toEqual([
      [1, "{one}"],
      [2, "{two}"],
    ]);
  });
});

describe("a failing write", () => {
  it("is reported once, not on every retry", async () => {
    invoked.mockRejectedValue(new Error("larger than 1048576 bytes"));

    saveLayoutSoon(1, "{huge}");
    await flushLayout();
    saveLayoutSoon(1, "{huge again}");
    await flushLayout();

    // Without this the ceiling would silently stop every autosave from then
    // on, for as long as the app runs, and say nothing.
    expect(toasted).toHaveBeenCalledTimes(1);
    expect(toasted).toHaveBeenCalledWith(expect.stringContaining("could not be saved"), "danger");
  });

  it("does not throw out of the debounce timer", async () => {
    invoked.mockRejectedValue(new Error("database is locked"));
    saveLayoutSoon(1, "{x}");
    expect(() => vi.advanceTimersByTime(LAYOUT_SAVE_DEBOUNCE_MS)).not.toThrow();
  });

  it("reports again after a write has succeeded in between", async () => {
    invoked.mockRejectedValueOnce(new Error("first"));
    saveLayoutSoon(1, "{a}");
    await flushLayout();

    invoked.mockResolvedValueOnce(true);
    saveLayoutSoon(1, "{b}");
    await flushLayout();

    invoked.mockRejectedValueOnce(new Error("second"));
    saveLayoutSoon(1, "{c}");
    await flushLayout();

    expect(toasted).toHaveBeenCalledTimes(2);
  });
});

describe("cancelLayoutWrite", () => {
  it("drops a queued write for that project", () => {
    saveLayoutSoon(1, "{gone}");
    cancelLayoutWrite(1);
    vi.advanceTimersByTime(LAYOUT_SAVE_DEBOUNCE_MS);
    expect(writes()).toEqual([]);
  });

  it("leaves another project's queued write alone", () => {
    saveLayoutSoon(1, "{keep}");
    cancelLayoutWrite(2);
    vi.advanceTimersByTime(LAYOUT_SAVE_DEBOUNCE_MS);
    expect(writes()).toEqual([[1, "{keep}"]]);
  });
});

describe("flushLayout", () => {
  it("writes what is waiting, immediately", async () => {
    saveLayoutSoon(7, "{now}");
    await flushLayout();
    expect(writes()).toEqual([[7, "{now}"]]);
  });

  it("does not write again once flushed, timer or no timer", async () => {
    saveLayoutSoon(7, "{now}");
    await flushLayout();
    vi.advanceTimersByTime(LAYOUT_SAVE_DEBOUNCE_MS * 4);
    expect(writes()).toEqual([[7, "{now}"]]);
  });

  it("is safe with nothing pending", async () => {
    await flushLayout();
    expect(writes()).toEqual([]);
  });
});
