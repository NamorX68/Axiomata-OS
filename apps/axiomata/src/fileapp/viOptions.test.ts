import { describe, expect, it } from "vitest";

import { applySet, type ViOptions } from "./viOptions";

const base: ViOptions = { wrap: false, lineNumbers: "absolute", list: false };

describe("applySet (:set, V6)", () => {
  it("switches wrap and list on, off and over", () => {
    expect(applySet(base, "wrap", true).wrap).toBe(true);
    expect(applySet({ ...base, wrap: true }, "wrap", false).wrap).toBe(false);
    expect(applySet(base, "list", "toggle").list).toBe(true);
  });

  it("combines number and relativenumber into one gutter", () => {
    expect(applySet(base, "relativenumber", true).lineNumbers).toBe("hybrid");
    expect(applySet(base, "number", false).lineNumbers).toBe("off");
    expect(applySet({ ...base, lineNumbers: "hybrid" }, "number", false).lineNumbers).toBe("relative");
    expect(applySet({ ...base, lineNumbers: "off" }, "relativenumber", "toggle").lineNumbers).toBe("relative");
    expect(applySet({ ...base, lineNumbers: "relative" }, "number", true).lineNumbers).toBe("hybrid");
  });

  it("leaves the other options alone", () => {
    expect(applySet(base, "wrap", true)).toEqual({ ...base, wrap: true });
  });
});
