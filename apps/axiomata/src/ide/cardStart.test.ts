import { describe, expect, it } from "vitest";
import type { IdeProject } from "../core/backend";
import { canStart, startableProjects } from "./cardStart";

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
