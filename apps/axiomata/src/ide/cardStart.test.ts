import { describe, expect, it } from "vitest";
import type { IdeProject } from "../core/backend";
import { canReview, canStart, canTakeOver, defaultTakeOverMessage, startableProjects } from "./cardStart";

const project = (id: number, opened: string | null, exists = true): IdeProject => ({
  id,
  name: `p${id}`,
  repo_root: `/p${id}`,
  layout_json: null,
  created_at: "2026-01-01T00:00:00Z",
  last_opened_at: opened,
  root_exists: exists,
});

describe("canStart", () => {
  it("is only for a card that is ready", () => {
    expect(canStart({ state: "ready" })).toBe(true);
    for (const state of ["proposed", "blocked", "working", "in_review", "verified", "failed"] as const) {
      expect(canStart({ state }), state).toBe(false);
    }
  });
});

describe("startableProjects", () => {
  it("offers the most recently opened project first and skips a missing folder", () => {
    const list = [
      project(1, "2026-02-01T00:00:00Z"),
      project(2, "2026-03-01T00:00:00Z"),
      project(3, null),
      project(4, "2026-04-01T00:00:00Z", false),
    ];
    expect(startableProjects(list).map((p) => p.id)).toEqual([2, 1, 3]);
  });
});

describe("review and take-over", () => {
  it("a review is started for a card in review, a take-over for a signed-off one", () => {
    expect(canReview({ state: "in_review" })).toBe(true);
    expect(canTakeOver({ state: "verified" })).toBe(true);
    for (const state of ["ready", "working", "verified", "taken_over", "failed"] as const) {
      expect(canReview({ state }), `review ${state}`).toBe(false);
    }
    for (const state of ["ready", "working", "in_review", "taken_over", "failed"] as const) {
      expect(canTakeOver({ state }), `take-over ${state}`).toBe(false);
    }
  });

  it("proposes the card's number and first line as the commit message", () => {
    expect(defaultTakeOverMessage({ id: 56, title: "Editor: Textgröße erhöhen" })).toBe("#56 Editor: Textgröße erhöhen");
    expect(defaultTakeOverMessage({ id: 7, title: "  First\nsecond " })).toBe("#7 First");
  });
});
