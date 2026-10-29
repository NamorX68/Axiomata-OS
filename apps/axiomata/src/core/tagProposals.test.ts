import { get, writable } from "svelte/store";
import { describe, expect, it, vi } from "vitest";

import type { TagProposal } from "./backend";
import { normaliseTag, refreshTagProposals, tagProblem, waitingLabel, watchTagProposals } from "./tagProposals";

const proposal = (id: number): TagProposal => ({
  id,
  tag: `t${id}`,
  description: "",
  notes: [],
  proposed_by: "a skill run",
  proposed_at: "2026-10-29T00:00:00Z",
});

describe("tag proposals", () => {
  it("checks a typed tag by the core's rules", () => {
    const list = [{ tag: "rust", description: "" }];
    expect(normaliseTag("  #Rust/Async ")).toBe("rust/async");
    expect(tagProblem("#Über_2-x", list)).toBeNull();
    expect(tagProblem("", list)).toMatch(/Type/);
    expect(tagProblem("a b", list)).toMatch(/Letters/);
    expect(tagProblem("2026", list)).toMatch(/digits/);
    expect(tagProblem("a//b", list)).toMatch(/between/);
    expect(tagProblem("_x", list)).toMatch(/Start/);
    expect(tagProblem("-x", list)).toMatch(/Start/);
    expect(tagProblem("Rust", list)).toMatch(/already/);
    expect(tagProblem("rust", list, "rust")).toBeNull();
    expect(tagProblem("x".repeat(65), list)).toMatch(/64/);
  });

  it("keeps the last list when a read fails, and polls until stopped", async () => {
    vi.useFakeTimers();
    const store = writable<TagProposal[]>([proposal(1)]);
    await refreshTagProposals(() => Promise.reject(new Error("offline")), store);
    expect(get(store)).toHaveLength(1);
    let reads = 0;
    const stop = watchTagProposals(
      async () => {
        reads += 1;
        return [proposal(1), proposal(2)];
      },
      1000,
      store,
    );
    await vi.advanceTimersByTimeAsync(2500);
    stop();
    await vi.advanceTimersByTimeAsync(5000);
    expect(reads).toBe(3);
    expect(get(store)).toHaveLength(2);
    vi.useRealTimers();
  });

  it("says how many wait", () => {
    expect(waitingLabel(1)).toBe("A tag proposal waits for you");
    expect(waitingLabel(3)).toBe("3 tag proposals wait for you");
  });
});
