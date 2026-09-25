/**
 * Direct tests of `regions.ts` (`docs/plans/editor.md`, ED3, V2): Visual
 * selections turned into regions, the lines they touch, and the size used to
 * replay a Visual change on fresh text with `.`.
 */

import { describe, expect, it } from "vitest";

import { docFrom } from "../testing";
import { pos } from "../position";
import {
  headForSize,
  prevChar,
  regionEnd,
  regionLines,
  regionStart,
  visualRegion,
  visualSize,
  wholeLines,
} from "./regions";

const TAB_SIZE = 4;

describe("visualRegion", () => {
  it("a characterwise selection includes the character under the head, forward or backward", () => {
    const doc = docFrom("abcdef");
    const forward = visualRegion(doc.store, "visual", pos(0, 1), pos(0, 3), false, TAB_SIZE);
    expect(forward).toEqual({ kind: "char", start: pos(0, 1), end: pos(0, 4) });
    const backward = visualRegion(doc.store, "visual", pos(0, 3), pos(0, 1), false, TAB_SIZE);
    expect(backward).toEqual({ kind: "char", start: pos(0, 1), end: pos(0, 4) });
  });

  it("a linewise selection spans whichever of anchor/head is first to last", () => {
    const doc = docFrom("a\nb\nc\nd");
    const region = visualRegion(doc.store, "visualLine", pos(2, 0), pos(0, 0), false, TAB_SIZE);
    expect(region).toEqual({ kind: "line", first: 0, last: 2 });
  });

  it("a block selection spans display columns, min/max of anchor and head", () => {
    const doc = docFrom("abcdef\nabcdef");
    const region = visualRegion(doc.store, "visualBlock", pos(0, 3), pos(1, 1), false, TAB_SIZE);
    expect(region).toEqual({ kind: "block", first: 0, last: 1, left: 1, right: 3 });
  });

  it("a block selection after $ (toEnd) reaches every line's end", () => {
    const doc = docFrom("ab\nabcdef");
    const region = visualRegion(doc.store, "visualBlock", pos(0, 0), pos(1, 0), true, TAB_SIZE);
    expect(region).toEqual({ kind: "block", first: 0, last: 1, left: 0, right: Infinity });
  });
});

describe("regionLines / wholeLines", () => {
  it("a characterwise region that ends at a line's start leaves that line out", () => {
    const region = { kind: "char" as const, start: pos(0, 2), end: pos(2, 0) };
    expect(regionLines(region)).toEqual({ first: 0, last: 1 });
  });

  it("a characterwise region that ends mid-line includes that line", () => {
    const region = { kind: "char" as const, start: pos(0, 2), end: pos(2, 3) };
    expect(regionLines(region)).toEqual({ first: 0, last: 2 });
  });

  it("wholeLines turns any region into the same lines, whole", () => {
    const region = { kind: "char" as const, start: pos(1, 2), end: pos(3, 1) };
    expect(wholeLines(region)).toEqual({ kind: "line", first: 1, last: 3 });
  });
});

describe("regionStart / regionEnd", () => {
  it("a line region starts at column 0 and ends at the last line's length", () => {
    const doc = docFrom("aaa\nbb\nc");
    const region = { kind: "line" as const, first: 0, last: 2 };
    expect(regionStart(doc.store, region, TAB_SIZE)).toEqual(pos(0, 0));
    expect(regionEnd(doc.store, region)).toEqual(pos(2, 1));
  });

  it("a block region starts at the left display column of its first line", () => {
    const doc = docFrom("abcdef");
    const region = { kind: "block" as const, first: 0, last: 0, left: 2, right: 4 };
    expect(regionStart(doc.store, region, TAB_SIZE)).toEqual(pos(0, 2));
  });
});

describe("visualSize / headForSize — for dot-repeat", () => {
  it("measures a one-line characterwise selection by its width", () => {
    const region = { kind: "char" as const, start: pos(0, 1), end: pos(0, 4) };
    expect(visualSize("visual", region)).toEqual({ mode: "visual", lines: 1, width: 3 });
  });

  it("measures a multi-line characterwise selection by its end column", () => {
    const region = { kind: "char" as const, start: pos(0, 1), end: pos(2, 5) };
    expect(visualSize("visual", region)).toEqual({ mode: "visual", lines: 3, width: 5 });
  });

  it("headForSize reselects the same width from a new cursor, clamped to the line", () => {
    const doc = docFrom("abcdefgh");
    const size = { mode: "visual" as const, lines: 1, width: 3 };
    expect(headForSize(doc.store, pos(0, 2), size)).toEqual(pos(0, 4));
  });

  it("headForSize on a short line clamps to the last character", () => {
    const doc = docFrom("ab\nxyz");
    const size = { mode: "visual" as const, lines: 1, width: 10 };
    expect(headForSize(doc.store, pos(0, 0), size)).toEqual(pos(0, 1));
  });

  it("headForSize for a linewise size reaches the same number of lines below", () => {
    const doc = docFrom("a\nb\nc\nd\ne");
    const size = { mode: "visualLine" as const, lines: 3, width: 0 };
    expect(headForSize(doc.store, pos(1, 0), size)).toEqual(pos(3, 0));
  });
});

describe("prevChar", () => {
  it("steps left within a line", () => {
    const doc = docFrom("abc");
    expect(prevChar(doc.store, pos(0, 2))).toEqual(pos(0, 1));
  });

  it("crosses a line break onto the previous line's last character", () => {
    const doc = docFrom("abc\nxyz");
    expect(prevChar(doc.store, pos(1, 0))).toEqual(pos(0, 2));
  });

  it("stays put at the very start of the text", () => {
    const doc = docFrom("abc");
    expect(prevChar(doc.store, pos(0, 0))).toEqual(pos(0, 0));
  });
});
