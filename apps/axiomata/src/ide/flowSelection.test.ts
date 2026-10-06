import { describe, expect, it } from "vitest";

import type { BoardPlan } from "../core/backend";
import { resolvePlan } from "./flowSelection";

const plan = (id: number, status: BoardPlan["status"]) => ({ id, status, name: `p${id}` }) as BoardPlan;

describe("the plan the Flow shows", () => {
  it("is the one picked while it exists, else the newest draft, else the newest", () => {
    const plans = [plan(1, "closed"), plan(2, "approved"), plan(3, "draft"), plan(4, "approved")];
    expect(resolvePlan(plans, 2)?.id).toBe(2);
    expect(resolvePlan(plans, null)?.id).toBe(3);
    expect(resolvePlan(plans, 99)?.id).toBe(3);
    expect(resolvePlan([plan(1, "closed"), plan(2, "approved")], null)?.id).toBe(2);
    expect(resolvePlan([], null)).toBeNull();
  });
});
