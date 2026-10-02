import { describe, expect, it } from "vitest";

import { loadAppGroups, appGroups } from "./appGroups";
import { get } from "svelte/store";
import { migrateRingTypes } from "./ringTypes";

describe("ring type migration", () => {
  it("maps the retired Editor entry onto Studio and drops duplicates, keeping order", () => {
    expect(migrateRingTypes(["terminal", "view:editor", "view:ide", "kanban"])).toEqual(["terminal", "view:ide", "kanban"]);
    expect(migrateRingTypes(["view:editor"])).toEqual(["view:ide"]);
  });

  it("applies to the members of a builtin group on load, but not a user group's paths", () => {
    loadAppGroups([
      { id: "a", side: "builtin", name: "A", glyph: "group", members: ["view:editor", "view:ide"] },
      { id: "b", side: "user", name: "B", glyph: "group", members: ["view:editor"] },
    ]);
    const [a, b] = get(appGroups);
    expect(a.members).toEqual(["view:ide"]);
    expect(b.members).toEqual(["view:editor"]);
  });
});
