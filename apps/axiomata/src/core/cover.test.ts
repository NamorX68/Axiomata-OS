import { get } from "svelte/store";
import { describe, expect, it } from "vitest";

import { coverGround, groundCovered } from "./cover";

describe("coverGround", () => {
  it("covers the ground while a hold is open", () => {
    expect(get(groundCovered)).toBe(false);
    const release = coverGround();
    expect(get(groundCovered)).toBe(true);
    release();
    expect(get(groundCovered)).toBe(false);
  });

  it("releases the ground only when every hold is gone", () => {
    const first = coverGround();
    const second = coverGround();
    first();
    expect(get(groundCovered)).toBe(true);
    second();
    expect(get(groundCovered)).toBe(false);
  });

  it("gives a hold back once even when released twice", () => {
    const first = coverGround();
    const second = coverGround();
    first();
    first();
    expect(get(groundCovered)).toBe(true);
    second();
    expect(get(groundCovered)).toBe(false);
  });
});
