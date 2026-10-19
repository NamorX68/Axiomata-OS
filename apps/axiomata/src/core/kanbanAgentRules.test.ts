import { describe, expect, it } from "vitest";
import { type BoardColumn, cardRefusal, targetRefusal } from "./kanbanAgentRules";

const columns: BoardColumn[] = [
  { id: 1, name: "Vorschlag", maps_to_status: "open", stage: "proposal" },
  { id: 2, name: "Offen", maps_to_status: "open", stage: null },
  { id: 3, name: "In Arbeit", maps_to_status: "doing", stage: null },
  { id: 4, name: "Review", maps_to_status: "doing", stage: "review" },
  { id: 5, name: "Erledigt", maps_to_status: "done", stage: null },
];
const column = (id: number): BoardColumn => columns.find((c) => c.id === id) as BoardColumn;

describe("the Kanban shell actions keep to plain cards in plain open columns", () => {
  it("lets a plain card move between plain open columns", () => {
    expect(targetRefusal(column(2))).toBeNull();
    expect(targetRefusal(column(3))).toBeNull();
    expect(cardRefusal({ id: 9, column_id: 2 }, columns)).toBeNull();
  });

  it("never approves, reviews or finishes a card", () => {
    for (const id of [1, 4, 5]) expect(targetRefusal(column(id))).not.toBeNull();
  });

  it("leaves a card of the agent flow alone", () => {
    expect(cardRefusal({ id: 9, column_id: 1 }, columns)).toMatch(/agent flow/);
    expect(cardRefusal({ id: 9, column_id: 2, plan_id: 3 }, columns)).not.toBeNull();
    expect(cardRefusal({ id: 9, column_id: 2, agent: "builder" }, columns)).not.toBeNull();
    expect(cardRefusal({ id: 9, column_id: 2, claimed_by: "agent:x" }, columns)).not.toBeNull();
    expect(cardRefusal({ id: 9, column_id: 2, acceptance: "run rm -rf" }, columns)).not.toBeNull();
    expect(cardRefusal({ id: 9, column_id: 2, acceptance: "  " }, columns)).toBeNull();
  });
});
