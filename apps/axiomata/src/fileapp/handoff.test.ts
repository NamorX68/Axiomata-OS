import { get } from "svelte/store";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { emit } from "../core/bus";
import { handoffs, handToFileApp, takeHandoffs, type Handoff } from "./handoff";

vi.mock("../core/bus", () => ({ emit: vi.fn() }));

const emitMock = vi.mocked(emit);

const fileHandoff: Handoff = { file: { root: "workspace", rel: "a.md" }, handed: null };
const noteHandoff: Handoff = { file: null, handed: null };

beforeEach(() => {
  emitMock.mockClear();
  // Every prior hand-over the queue may still hold from an earlier test.
  handoffs.set([]);
});

describe("handToFileApp", () => {
  it("queues the hand-over and tells the shell to show the file app", () => {
    handToFileApp(fileHandoff);
    expect(get(handoffs)).toEqual([fileHandoff]);
    expect(emitMock).toHaveBeenCalledWith("shell:editor");
  });

  it("appends rather than replacing an already-waiting hand-over", () => {
    handToFileApp(fileHandoff);
    handToFileApp(noteHandoff);
    expect(get(handoffs)).toEqual([fileHandoff, noteHandoff]);
    expect(emitMock).toHaveBeenCalledTimes(2);
  });
});

describe("takeHandoffs", () => {
  it("drains every waiting hand-over, oldest first, and empties the queue", () => {
    handToFileApp(fileHandoff);
    handToFileApp(noteHandoff);
    expect(takeHandoffs()).toEqual([fileHandoff, noteHandoff]);
    expect(get(handoffs)).toEqual([]);
  });

  it("returns an empty list, and leaves the queue empty, when nothing is waiting", () => {
    expect(takeHandoffs()).toEqual([]);
    expect(get(handoffs)).toEqual([]);
  });

  it("does not hand the same entry out twice across two drains", () => {
    handToFileApp(fileHandoff);
    expect(takeHandoffs()).toEqual([fileHandoff]);
    expect(takeHandoffs()).toEqual([]);
  });
});
