import { describe, expect, it } from "vitest";

import { resizeDelta } from "./resize";

describe("resizeDelta", () => {
  it("grows with the pointer on the right and bottom edges", () => {
    expect(resizeDelta("e", 10, 20)).toEqual({ dw: 10, dh: 0 });
    expect(resizeDelta("s", 10, 20)).toEqual({ dw: 0, dh: 20 });
    expect(resizeDelta("se", 10, 20)).toEqual({ dw: 10, dh: 20 });
  });

  it("grows against the pointer on the left and top edges", () => {
    expect(resizeDelta("w", 10, 20)).toEqual({ dw: -10, dh: 0 });
    expect(resizeDelta("n", 10, 20)).toEqual({ dw: 0, dh: -20 });
    expect(resizeDelta("sw", 10, 20)).toEqual({ dw: -10, dh: 20 });
  });

  it("pulls both sides from an upper corner", () => {
    expect(resizeDelta("ne", 10, -20)).toEqual({ dw: 10, dh: 20 });
    expect(resizeDelta("nw", -10, -20)).toEqual({ dw: 10, dh: 20 });
  });
});
