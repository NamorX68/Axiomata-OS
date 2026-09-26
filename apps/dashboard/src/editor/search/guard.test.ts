import { afterEach, describe, expect, it, vi } from "vitest";

import { Rope } from "../rope";
import { SearchGuard, SearchPending } from "./guard";
import { FakeSearchWorker } from "./testing";
import { allMatches, spansLines } from "./matches";

class FakeWorker extends FakeSearchWorker {}

afterEach(() => {
  vi.useRealTimers();
});

describe("allMatches", () => {
  it("finds matches line by line, as offsets into the joined text", () => {
    const found = allMatches("ab\nab", /b/gu, 10);
    expect([...found.offsets]).toEqual([1, 2, 4, 5]);
    expect(found.count).toBe(2);
  });

  it("lets a pattern that names a line break reach across lines", () => {
    expect(spansLines(/b\na/gu)).toBe(true);
    expect(spansLines(/\s/gu)).toBe(false);
    expect([...allMatches("ab\nab", /b\na/gu, 10).offsets]).toEqual([1, 4]);
    // Line by line, `\s` never sees the break.
    expect(allMatches("a\nb", /a\sb/gu, 10).count).toBe(0);
  });

  it("keeps only the first matches up to the limit, but counts them all", () => {
    const found = allMatches("aaaa", /a/gu, 2);
    expect([...found.offsets]).toEqual([0, 1, 1, 2]);
    expect(found.count).toBe(4);
    expect(found.truncated).toBe(true);
  });
});

describe("SearchGuard", () => {
  it("throws SearchPending until the worker has run the pattern, then knows the matches", async () => {
    const worker = new FakeWorker();
    const guard = new SearchGuard(() => worker);
    const rope = Rope.of("foo bar\nfoo");
    let pending: unknown;
    try {
      guard.require(rope, /foo/gu);
    } catch (err) {
      pending = err;
    }
    expect(pending).toBeInstanceOf(SearchPending);
    await (pending as SearchPending).done;
    const verdict = guard.require(rope, /foo/gu);
    expect(verdict.ok && verdict.count).toBe(2);
    expect(worker.seen).toHaveLength(1);
  });

  it("keeps verdicts per text version: an edit asks again", async () => {
    const worker = new FakeWorker();
    const guard = new SearchGuard(() => worker);
    const first = Rope.of("a");
    await guard.request(first, /a/gu);
    const second = first.replace({ start: { line: 0, col: 1 }, end: { line: 0, col: 1 } }, "a").rope;
    expect(guard.verdict(second, /a/gu)).toBeNull();
    await guard.request(second, /a/gu);
    const verdict = guard.verdict(second, /a/gu);
    expect(verdict?.ok && verdict.count).toBe(2);
    expect(guard.verdict(first, /a/gu)).not.toBeNull();
  });

  it("asks once for the same pattern and text, however often it is wanted", async () => {
    const worker = new FakeWorker();
    const guard = new SearchGuard(() => worker);
    const rope = Rope.of("x");
    await Promise.all([guard.request(rope, /x/gu), guard.request(rope, /x/gu), guard.request(rope, /x/gu)]);
    expect(worker.seen).toHaveLength(1);
  });

  it("calls a pattern too expensive when the worker does not answer in time, and starts a fresh worker", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const workers: FakeWorker[] = [];
    const guard = new SearchGuard(() => {
      const w = new FakeWorker((req) => req.source === "(a+)+$");
      workers.push(w);
      return w;
    }, 1000);
    const rope = Rope.of("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab");
    const done = guard.request(rope, /(a+)+$/gu);
    const next = guard.request(rope, /b/gu);
    await vi.advanceTimersByTimeAsync(1000);
    await done;
    const verdict = guard.verdict(rope, /(a+)+$/gu);
    expect(verdict).toMatchObject({ ok: false, reason: "timeout" });
    expect(workers[0].terminated).toBe(true);
    // The queue goes on in a new worker.
    await vi.advanceTimersByTimeAsync(0);
    await next;
    expect(workers).toHaveLength(2);
    expect(guard.verdict(rope, /b/gu)?.ok).toBe(true);
  });

  it("drops a waiting job for the same pattern on an older text", async () => {
    const worker = new FakeWorker((req) => req.source === "block");
    const guard = new SearchGuard(() => worker, 60_000);
    const a = Rope.of("one");
    const b = Rope.of("two");
    void guard.request(a, /block/gu); // occupies the worker
    const old = guard.request(a, /o/gu);
    void guard.request(b, /o/gu);
    await old; // settled without a verdict: the caller looks again
    expect(guard.verdict(a, /o/gu)).toBeNull();
    guard.dispose();
  });

  it("reports a pattern the worker cannot compile as an error", async () => {
    const guard = new SearchGuard(() => new FakeWorker());
    const rope = Rope.of("x");
    const re = /x/gu;
    Object.defineProperty(re, "source", { value: "(" });
    await guard.request(rope, re);
    expect(guard.verdict(rope, re)).toMatchObject({ ok: false, reason: "error" });
  });

  it("tells listeners about every verdict", async () => {
    const guard = new SearchGuard(() => new FakeWorker());
    const heard = vi.fn();
    guard.onVerdict(heard);
    await guard.request(Rope.of("x"), /x/gu);
    expect(heard).toHaveBeenCalledTimes(1);
  });

  it("dispose settles the running job and every queued one, without recording a verdict", async () => {
    const worker = new FakeWorker((req) => req.source === "hang");
    const guard = new SearchGuard(() => worker);
    const rope = Rope.of("x");
    const running = guard.request(rope, /hang/gu); // occupies the worker forever
    const queued = guard.request(rope, /other/gu); // never gets its turn
    guard.dispose();
    // Neither promise is left hanging: both settle once disposed.
    await Promise.all([running, queued]);
    expect(guard.verdict(rope, /hang/gu)).toBeNull();
    expect(guard.verdict(rope, /other/gu)).toBeNull();
    expect(worker.terminated).toBe(true);
  });

  it("treats the worker's own onerror as a failure of the running job, not a hang", async () => {
    const worker = new FakeWorker((req) => req.source === "x");
    const guard = new SearchGuard(() => worker);
    const rope = Rope.of("x");
    const done = guard.request(rope, /x/gu);
    worker.onerror?.(new Error("boom"));
    await done;
    expect(guard.verdict(rope, /x/gu)).toMatchObject({ ok: false, reason: "error" });
  });
});
