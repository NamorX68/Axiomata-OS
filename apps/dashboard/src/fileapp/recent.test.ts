import { describe, expect, it } from "vitest";

import { MAX_RECENT, withRecent } from "./recent";

describe("withRecent", () => {
  it("puts the file first without duplicating it", () => {
    const list = [
      { root: "workspace", rel: "a.md" },
      { root: "project:1", rel: "b.rs" },
    ];
    expect(withRecent(list, { root: "project:1", rel: "b.rs" })).toEqual([
      { root: "project:1", rel: "b.rs" },
      { root: "workspace", rel: "a.md" },
    ]);
  });

  it("tells the same path under different roots apart and caps the list", () => {
    const many = Array.from({ length: MAX_RECENT }, (_, i) => ({ root: "workspace", rel: `${i}.md` }));
    const next = withRecent(many, { root: "project:1", rel: "0.md" });
    expect(next).toHaveLength(MAX_RECENT);
    expect(next[0]).toEqual({ root: "project:1", rel: "0.md" });
    expect(next).toContainEqual({ root: "workspace", rel: "0.md" });
  });
});
