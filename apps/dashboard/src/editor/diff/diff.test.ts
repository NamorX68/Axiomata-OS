import { describe, expect, it } from "vitest";

import { DiffHighlight } from "./highlight";
import { hunksFromTexts, parseHunkHeader, type DiffHunk } from "./hunks";
import { DiffModel, NO_NEWLINE_NOTE, textLines, TRUNCATED_NOTE, UNFOLD_STEP, type FoldRow, type LineRow } from "./model";
import {
  changedLineNear,
  foldActionId,
  hunkActionId,
  parseFoldActionId,
  parseHunkActionId,
  splitPanes,
  unifiedPane,
} from "./view";
import { tokenize, wordChanges } from "./words";

const lines = (n: number, prefix = "l") => Array.from({ length: n }, (_, i) => `${prefix}${i + 1}`);

describe("parseHunkHeader", () => {
  it("reads both ranges, defaulting a missing count to one, and the context", () => {
    expect(parseHunkHeader("@@ -3,4 +5,6 @@ fn main() {")).toEqual({
      oldStart: 3,
      oldCount: 4,
      newStart: 5,
      newCount: 6,
      context: "fn main() {",
    });
    expect(parseHunkHeader("@@ -1 +1 @@")).toMatchObject({ oldCount: 1, newCount: 1, context: "" });
    expect(parseHunkHeader("not a header")).toBeNull();
  });
});

describe("hunksFromTexts (H7)", () => {
  it("makes git-shaped hunks with three lines of context", () => {
    const a = lines(20);
    const b = [...a];
    b[9] = "changed";
    const hunks = hunksFromTexts(a, b);
    expect(hunks).toHaveLength(1);
    expect(hunks[0].header).toBe("@@ -7,7 +7,7 @@");
    expect(hunks[0].lines.map((l) => l.kind)).toEqual([
      "context",
      "context",
      "context",
      "remove",
      "add",
      "context",
      "context",
      "context",
    ]);
    expect(hunks[0].lines[3]).toEqual({ kind: "remove", oldLine: 10, newLine: null, text: "l10" });
    expect(hunks[0].lines[4]).toEqual({ kind: "add", oldLine: null, newLine: 10, text: "changed" });
  });

  it("merges changes up to six lines apart and splits those further apart", () => {
    const a = lines(40);
    const near = [...a];
    near[5] = "x";
    near[11] = "y";
    expect(hunksFromTexts(a, near)).toHaveLength(1);
    const far = [...a];
    far[5] = "x";
    far[30] = "y";
    expect(hunksFromTexts(a, far)).toHaveLength(2);
  });

  it("names the line before an empty range, as git does", () => {
    expect(hunksFromTexts([], ["a", "b"])[0].header).toBe("@@ -0,0 +1,2 @@");
    expect(hunksFromTexts(["a", "b"], ["a"])[0].header).toBe("@@ -1,2 +1 @@");
    expect(hunksFromTexts(["a"], ["a"])).toEqual([]);
  });
});

describe("wordChanges (H4)", () => {
  it("splits into words, spaces and single punctuation", () => {
    expect(tokenize("let ä_1 = f(x);")).toEqual(["let", " ", "ä_1", " ", "=", " ", "f", "(", "x", ")", ";"]);
  });

  it("marks only the words that changed, on each side", () => {
    const c = wordChanges("let total = price * count;", "let total = price * amount;");
    expect(c).toEqual({ old: [{ from: 20, to: 25 }], new: [{ from: 20, to: 26 }] });
  });

  it("marks nothing for unrelated lines or very long ones", () => {
    expect(wordChanges("fn alpha() {}", "// a completely different comment")).toBeNull();
    expect(wordChanges("x".repeat(2000), "y")).toBeNull();
    expect(wordChanges("same", "same")).toEqual({ old: [], new: [] });
  });
});

/** A diff of a 30-line file where line 15 changed, as git would send it. */
function midFileHunk(): DiffHunk {
  return {
    header: "@@ -12,7 +12,7 @@ fn middle",
    lines: [
      { kind: "context", oldLine: 12, newLine: 12, text: "l12" },
      { kind: "context", oldLine: 13, newLine: 13, text: "l13" },
      { kind: "context", oldLine: 14, newLine: 14, text: "l14" },
      { kind: "remove", oldLine: 15, newLine: null, text: "let a = 1;" },
      { kind: "add", oldLine: null, newLine: 15, text: "let a = 2;" },
      { kind: "context", oldLine: 16, newLine: 16, text: "l16" },
      { kind: "context", oldLine: 17, newLine: 17, text: "l17" },
      { kind: "context", oldLine: 18, newLine: 18, text: "l18" },
    ],
  };
}

function thirtyLines(): { oldLines: string[]; newLines: string[] } {
  const oldLines = lines(30);
  oldLines[14] = "let a = 1;";
  const newLines = [...oldLines];
  newLines[14] = "let a = 2;";
  return { oldLines, newLines };
}

describe("DiffModel", () => {
  it("folds the unchanged lines around a hunk, with git's context as the label", () => {
    const model = new DiffModel({ hunks: [midFileHunk()], ...thirtyLines() });
    const rows = model.unified();
    expect(rows[0]).toEqual({ type: "fold", gap: 0, hidden: 11, label: "fn middle", actions: ["all"] });
    expect(rows[rows.length - 1]).toMatchObject({ type: "fold", gap: 1, hidden: 12, label: "" });
    const changed = rows.filter((r): r is LineRow => r.type === "line" && r.kind !== "context");
    expect(changed.map((r) => [r.kind, r.oldLine, r.newLine])).toEqual([
      ["remove", 14, null],
      ["add", null, 14],
    ]);
    expect(changed[0].words).toEqual([{ from: 8, to: 9 }]);
  });

  it("unfolds a gap in steps from either end, then entirely", () => {
    const oldLines = lines(100);
    const newLines = [...oldLines];
    newLines[79] = "changed";
    const hunks = hunksFromTexts(oldLines, newLines);
    const model = new DiffModel({ hunks, oldLines, newLines });
    const fold = () => model.unified()[0];
    expect(fold()).toMatchObject({ type: "fold", hidden: 76, actions: ["up", "all"] });
    model.unfold(0, "up");
    expect(fold()).toMatchObject({ type: "fold", hidden: 76 - UNFOLD_STEP });
    const rows = model.unified();
    // The unfolded lines sit right above the hunk, numbered on both sides.
    expect(rows[1]).toMatchObject({ type: "line", kind: "context", oldLine: 56, newLine: 56, text: "l57", hunk: null });
    model.unfold(0, "all");
    expect(model.unified()[0]).toMatchObject({ type: "line", oldLine: 0, text: "l1" });
  });

  it("shows the whole file and folds back", () => {
    const model = new DiffModel({ hunks: [midFileHunk()], ...thirtyLines() });
    model.setWholeFile(true);
    expect(model.unified().every((r) => r.type === "line")).toBe(true);
    expect(model.unified()).toHaveLength(31);
    model.setWholeFile(false);
    expect(model.unified()[0].type).toBe("fold");
  });

  it("cannot unfold without the full text, and says nothing about the end", () => {
    const model = new DiffModel({ hunks: [midFileHunk()] });
    expect(model.unified()[0]).toMatchObject({ type: "fold", hidden: 11, actions: [] });
    expect(model.unified()[model.unified().length - 1]).toMatchObject({ type: "line", text: "l18" });
    model.unfold(0, "all");
    expect(model.unified()[0].type).toBe("fold");
  });

  it("notes a missing final newline and a cut-off diff", () => {
    const hunk: DiffHunk = {
      header: "@@ -0,0 +1,2 @@",
      lines: [
        { kind: "add", oldLine: null, newLine: 1, text: "a" },
        { kind: "add", oldLine: null, newLine: 2, text: "b" },
        { kind: "no_newline", oldLine: null, newLine: null, text: "" },
      ],
    };
    const rows = new DiffModel({ hunks: [hunk], newLines: ["a", "b"], oldLines: null, truncated: true }).unified();
    expect(rows.map((r) => r.type)).toEqual(["line", "line", "note", "note"]);
    expect(rows[2]).toEqual({ type: "note", text: NO_NEWLINE_NOTE });
    expect(rows[3]).toEqual({ type: "note", text: TRUNCATED_NOTE });
  });

  it("pairs a change block side by side, blank where one side has fewer lines", () => {
    const hunk: DiffHunk = {
      header: "@@ -1,3 +1,2 @@",
      lines: [
        { kind: "remove", oldLine: 1, newLine: null, text: "a" },
        { kind: "remove", oldLine: 2, newLine: null, text: "b" },
        { kind: "add", oldLine: null, newLine: 1, text: "A" },
        { kind: "context", oldLine: 3, newLine: 2, text: "c" },
      ],
    };
    const split = new DiffModel({ hunks: [hunk] }).split();
    expect(split.map((r) => (r.type === "pair" ? [r.left?.text ?? null, r.right?.text ?? null] : r.type))).toEqual([
      ["a", "A"],
      ["b", null],
      ["c", "c"],
    ]);
  });

  it("counts lines the way git does", () => {
    expect(textLines("a\nb\n")).toEqual(["a", "b"]);
    expect(textLines("a\r\nb")).toEqual(["a", "b"]);
    expect(textLines("")).toEqual([""]);
  });

  it("shows a pure-deletion hunk with no gap either side when it covers the whole file", () => {
    // A deleted file: no `newLines` at all, every line of the hunk a removal.
    const oldLines = lines(4);
    const hunk: DiffHunk = {
      header: "@@ -1,4 +0,0 @@",
      lines: oldLines.map((text, i) => ({ kind: "remove" as const, oldLine: i + 1, newLine: null, text })),
    };
    const rows = new DiffModel({ hunks: [hunk], oldLines, newLines: null }).unified();
    expect(rows.every((r) => r.type === "line")).toBe(true);
    expect(rows.map((r) => (r as LineRow).kind)).toEqual(Array(4).fill("remove"));
  });

  it("falls back to the old side alone for gap text once the file has no new side", () => {
    // A deleted file where only the first three lines are inside the hunk;
    // the rest of the (now-gone) file is a trailing gap with nothing but the
    // old side to draw it from.
    const oldLines = lines(10);
    const hunk: DiffHunk = {
      header: "@@ -1,3 +0,0 @@",
      lines: [
        { kind: "remove", oldLine: 1, newLine: null, text: "l1" },
        { kind: "remove", oldLine: 2, newLine: null, text: "l2" },
        { kind: "remove", oldLine: 3, newLine: null, text: "l3" },
      ],
    };
    const model = new DiffModel({ hunks: [hunk], oldLines, newLines: null });
    model.setWholeFile(true);
    const rows = model.unified();
    const last = rows[rows.length - 1] as LineRow;
    expect(last).toMatchObject({ type: "line", kind: "context", oldLine: 9, text: "l10" });
  });

  it("unfolds a middle gap from each end independently, without reaching the other hunk", () => {
    const oldLines = lines(200);
    const newLines = [...oldLines];
    newLines[9] = "x";
    newLines[149] = "y";
    const hunks = hunksFromTexts(oldLines, newLines);
    expect(hunks).toHaveLength(2);
    const model = new DiffModel({ hunks, oldLines, newLines });
    const middleFold = () => model.unified().find((r): r is FoldRow => r.type === "fold" && r.gap === 1)!;

    expect(middleFold().actions).toEqual(["down", "up", "all"]);
    const initialHidden = middleFold().hidden;

    model.unfold(1, "down");
    expect(middleFold().hidden).toBe(initialHidden - UNFOLD_STEP);
    model.unfold(1, "up");
    expect(middleFold().hidden).toBe(initialHidden - 2 * UNFOLD_STEP);

    // The lines just below hunk 1 and just above hunk 2 are unfolded context,
    // numbered continuously — the other hunk's gap was untouched.
    const rows = model.unified();
    const foldIndex = rows.findIndex((r) => r.type === "fold" && r.gap === 1);
    expect(rows[foldIndex - 1]).toMatchObject({ type: "line", kind: "context" });
    expect(rows[foldIndex + 1]).toMatchObject({ type: "line", kind: "context" });
  });

  it("clamps repeated unfolding so a gap's two ends never overlap or go negative", () => {
    // A hand-built hunk starting at old/new line 26, so the leading gap has an
    // exact, known count (25) to unfold against; a two-line trailing gap
    // (lines 29-30) is left alone throughout, to show the clamp is local to
    // the gap being unfolded.
    const hunk: DiffHunk = {
      header: "@@ -26,3 +26,3 @@",
      lines: [
        { kind: "context", oldLine: 26, newLine: 26, text: "l26" },
        { kind: "remove", oldLine: 27, newLine: null, text: "old" },
        { kind: "add", oldLine: null, newLine: 27, text: "new" },
        { kind: "context", oldLine: 28, newLine: 28, text: "l28" },
      ],
    };
    const oldLines = lines(30);
    const newLines = [...oldLines];
    newLines[26] = "new";
    const model = new DiffModel({ hunks: [hunk], oldLines, newLines });
    const leadingFold = () => model.unified().find((r): r is FoldRow => r.type === "fold" && r.gap === 0);
    expect(leadingFold()).toMatchObject({ type: "fold", gap: 0, hidden: 25 });

    // "down" twice unfolds past the gap's own size; the second call must clamp
    // rather than push `top` beyond `count`.
    model.unfold(0, "down");
    expect(leadingFold()).toMatchObject({ type: "fold", gap: 0, hidden: 5 });
    model.unfold(0, "down");
    const rowsAfter = model.unified();
    expect(rowsAfter.some((r) => r.type === "fold" && r.gap === 0)).toBe(false);
    expect(rowsAfter[0]).toMatchObject({ type: "line", kind: "context", oldLine: 0, text: "l1" });
    // The unrelated trailing gap is still folded — clamping is per gap.
    expect(rowsAfter[rowsAfter.length - 1]).toMatchObject({ type: "fold", hidden: 2 });

    // A further "down" on the now-fully-shown leading gap is a no-op, not an
    // error or a negative `hidden`.
    model.unfold(0, "down");
    expect(model.unified().filter((r) => r.type === "line" && r.oldLine !== null && r.oldLine < 25)).toHaveLength(25);
  });

  it("pairs a removed and an added line across a no-newline note between them", () => {
    // What real git prints when the last line of both sides changed and
    // neither side ends in a newline: the note sits *between* the removal and
    // the addition. The split layout pairs the two lines like `markWords` does
    // and puts the notes after the pair.
    const hunk: DiffHunk = {
      header: "@@ -1 +1 @@",
      lines: [
        { kind: "remove", oldLine: 1, newLine: null, text: "let a = 1;" },
        { kind: "no_newline", oldLine: null, newLine: null, text: "" },
        { kind: "add", oldLine: null, newLine: 1, text: "let a = 2;" },
        { kind: "no_newline", oldLine: null, newLine: null, text: "" },
      ],
    };
    const split = new DiffModel({ hunks: [hunk] }).split();
    expect(split.map((r) => (r.type === "pair" ? [r.left?.text ?? null, r.right?.text ?? null] : r.type))).toEqual([
      ["let a = 1;", "let a = 2;"],
      "note",
      "note",
    ]);
    const pair = split[0] as { left: LineRow; right: LineRow };
    expect(pair.left.words).toEqual([{ from: 8, to: 9 }]);
    expect(pair.right.words).toEqual([{ from: 8, to: 9 }]);
  });
});

describe("hunksFromTexts round trip", () => {
  /** Rebuilds the new text from the old text plus the hunks, using each line's own position. */
  function applyHunks(oldLines: readonly string[], hunks: readonly DiffHunk[]): string[] {
    const out: string[] = [];
    let oldIdx = 0;
    for (const hunk of hunks) {
      for (const line of hunk.lines) {
        if (line.kind === "add") {
          out.push(line.text);
        } else {
          const target = line.oldLine! - 1;
          while (oldIdx < target) out.push(oldLines[oldIdx++]);
          if (line.kind === "context") out.push(oldLines[oldIdx]);
          oldIdx = target + 1;
        }
      }
    }
    while (oldIdx < oldLines.length) out.push(oldLines[oldIdx++]);
    return out;
  }

  it("reconstructs the new text from the old text and the produced hunks, for random inputs", () => {
    let seed = 42;
    const rand = () => (seed = (seed * 1103515245 + 12345) % 2 ** 31);
    const randomLines = (n: number) => Array.from({ length: n }, () => `l${rand() % 15}`);
    for (let round = 0; round < 100; round++) {
      const oldLines = randomLines(rand() % 30);
      const newLines = randomLines(rand() % 30);
      const hunks = hunksFromTexts(oldLines, newLines);
      expect(applyHunks(oldLines, hunks)).toEqual(newLines);
    }
  });

  it("round-trips a pure insertion and a pure deletion", () => {
    expect(applyHunks([], hunksFromTexts([], ["a", "b"]))).toEqual(["a", "b"]);
    expect(applyHunks(["a", "b"], hunksFromTexts(["a", "b"], []))).toEqual([]);
  });
});

describe("diff panes", () => {
  it("builds a unified document with both numbers, signs, word marks and fold buttons", () => {
    const model = new DiffModel({ hunks: [midFileHunk()], ...thirtyLines() });
    const pane = unifiedPane(model.unified(), UNFOLD_STEP);
    const text = pane.text.split("\n");
    expect(text[0]).toBe("");
    expect(text[4]).toBe("let a = 1;");
    const fold = pane.decorations.line(0);
    expect(fold).toMatchObject({ kind: "fold", label: "11 unchanged lines · fn middle" });
    expect(fold?.actions?.map((a) => a.id)).toEqual([foldActionId(0, "all")]);
    expect(pane.decorations.line(4)).toEqual({
      kind: "remove",
      gutter: "15    -",
      marks: [{ from: 8, to: 9, kind: "remove-word" }],
    });
    expect(pane.decorations.line(5)?.gutter).toBe("   15 +");
    expect(pane.decorations.gutterCells).toBe(9);
    expect(pane.sources[0]).toBeNull();
    expect(pane.sources[4]).toEqual({ side: "old", line: 14 });
    expect(pane.sources[5]).toEqual({ side: "new", line: 14 });
    expect(pane.hunkStarts).toEqual([1]);
  });

  it("keeps the split panes row for row, hunk starts included", () => {
    const hunks = hunksFromTexts(lines(40), ["new first", ...lines(40).slice(1, 30), "x", ...lines(40).slice(31)]);
    const model = new DiffModel({ hunks, oldLines: lines(40) });
    const { left, right } = splitPanes(model.split(), UNFOLD_STEP);
    expect(left.text.split("\n")).toHaveLength(right.text.split("\n").length);
    expect(left.hunkStarts).toEqual(right.hunkStarts);
    expect(left.hunkStarts).toHaveLength(2);
    expect(left.decorations.line(0)).toMatchObject({ kind: "remove", gutter: " 1 -" });
    expect(right.decorations.line(0)).toMatchObject({ kind: "add", gutter: " 1 +" });
  });

  it("finds the changed side's line to open, even from a removed line or a fold", () => {
    const model = new DiffModel({ hunks: [midFileHunk()], ...thirtyLines() });
    const pane = unifiedPane(model.unified(), UNFOLD_STEP);
    expect(changedLineNear(pane, 5)).toBe(14); // the added line itself
    expect(changedLineNear(pane, 4)).toBe(14); // the removed line: where it was replaced
    expect(changedLineNear(pane, 0)).toBe(11); // the fold before the hunk: its first line
    const deleted = new DiffModel({ hunks: hunksFromTexts(["a", "b"], []), oldLines: ["a", "b"], newLines: null });
    expect(changedLineNear(unifiedPane(deleted.unified(), UNFOLD_STEP), 0)).toBeNull();
  });

  it("puts a header row with a discard button above each hunk when asked", () => {
    const model = new DiffModel({ hunks: [midFileHunk()], ...thirtyLines() });
    const plain = unifiedPane(model.unified(), UNFOLD_STEP);
    const pane = unifiedPane(model.unified(), UNFOLD_STEP, { headers: ["@@ -12,7 +12,7 @@ fn middle"], discard: true });
    expect(pane.text.split("\n")).toHaveLength(plain.text.split("\n").length + 1);
    expect(pane.decorations.line(1)).toMatchObject({ kind: "hunk", label: "@@ -12,7 +12,7 @@ fn middle" });
    expect(pane.decorations.line(1)?.actions?.map((a) => a.id)).toEqual([hunkActionId(0, "discard")]);
    expect(pane.hunkStarts).toEqual([1]);
    expect(pane.hunkOf[1]).toBe(0);
    expect(pane.hunkOf[5]).toBe(0);
    expect(pane.hunkOf[0]).toBeNull();
    const { left, right } = splitPanes(model.split(), UNFOLD_STEP, { headers: ["@@ h @@"], discard: true });
    expect(left.decorations.line(1)?.actions).toEqual([]);
    expect(right.decorations.line(1)?.actions).toHaveLength(1);
    expect(parseHunkActionId(hunkActionId(3, "discard"))).toEqual({ hunk: 3, action: "discard" });
  });

  it("offers Stage or Unstage on a hunk for the git panel, instead of Discard", () => {
    const model = new DiffModel({ hunks: [midFileHunk()], ...thirtyLines() });
    const stage = unifiedPane(model.unified(), UNFOLD_STEP, { headers: ["@@ h @@"], discard: false, action: "stage" });
    expect(stage.decorations.line(1)?.actions).toMatchObject([{ id: hunkActionId(0, "stage"), label: "Stage" }]);
    const unstage = unifiedPane(model.unified(), UNFOLD_STEP, { headers: ["@@ h @@"], discard: false, action: "unstage" });
    expect(unstage.decorations.line(1)?.actions).toMatchObject([{ id: hunkActionId(0, "unstage"), label: "Unstage" }]);
    expect(parseHunkActionId(hunkActionId(2, "stage"))).toEqual({ hunk: 2, action: "stage" });
    expect(parseHunkActionId("hunk:1:nope")).toBeNull();
  });

  it("round-trips fold action ids", () => {
    expect(parseFoldActionId(foldActionId(3, "up"))).toEqual({ gap: 3, action: "up" });
    expect(parseFoldActionId("something else")).toBeNull();
  });

  it("shows a header with no discard button for a read-only diff (the file app's Compare)", () => {
    const model = new DiffModel({ hunks: [midFileHunk()], ...thirtyLines() });
    const pane = unifiedPane(model.unified(), UNFOLD_STEP, { headers: ["@@ -12,7 +12,7 @@ fn middle"], discard: false });
    expect(pane.decorations.line(1)).toMatchObject({ kind: "hunk", label: "@@ -12,7 +12,7 @@ fn middle" });
    expect(pane.decorations.line(1)?.actions).toEqual([]);
  });

  it("skips a header row for a hunk the headers array has nothing for, but still starts a new hunk", () => {
    const oldLines = lines(60);
    const newLines = [...oldLines];
    newLines[4] = "changed near the top";
    newLines[54] = "changed near the bottom";
    const hunks = hunksFromTexts(oldLines, newLines);
    expect(hunks).toHaveLength(2);
    const model = new DiffModel({ hunks, oldLines, newLines });
    const plain = unifiedPane(model.unified(), UNFOLD_STEP);
    // Only the first hunk has a header to show; the second's index is out of
    // range for a one-element `headers` array.
    const pane = unifiedPane(model.unified(), UNFOLD_STEP, { headers: [hunks[0].header], discard: true });
    // Only one header row is added.
    expect(pane.text.split("\n")).toHaveLength(plain.text.split("\n").length + 1);
    expect(pane.hunkStarts).toHaveLength(2);
    // The second hunk's start line has no header decoration — it goes
    // straight to the hunk's own first line rather than an empty header row.
    const secondStartDeco = pane.decorations.line(pane.hunkStarts[1]);
    expect(secondStartDeco?.kind).not.toBe("hunk");
    expect(pane.hunkOf[pane.hunkStarts[1]]).toBe(1);
  });

  it("gives a blank row (the missing side of a pure insertion) the same hunk index as its partner", () => {
    // "x" is a pure insertion inside the change block: the left (old) side has
    // no matching line there, only a blank row — it must still carry the
    // hunk index so ⌘⌫ (discard the hunk under the cursor) works from it too.
    const model = new DiffModel({ hunks: hunksFromTexts(["a", "b"], ["a", "x", "b"]), oldLines: ["a", "b"] });
    const { left, right } = splitPanes(model.split(), UNFOLD_STEP);
    const insertedRow = right.text.split("\n").indexOf("x");
    expect(insertedRow).toBeGreaterThanOrEqual(0);
    expect(left.decorations.line(insertedRow)).toMatchObject({ kind: "blank" });
    expect(left.hunkOf[insertedRow]).not.toBeNull();
    expect(left.hunkOf[insertedRow]).toBe(right.hunkOf[insertedRow]);
  });

  it("lines up hunkStarts and hunkOf on both sides of the split layout", () => {
    const oldLines = lines(60);
    const newLines = [...oldLines];
    newLines[4] = "changed near the top";
    newLines[54] = "changed near the bottom";
    const hunks = hunksFromTexts(oldLines, newLines);
    const model = new DiffModel({ hunks, oldLines, newLines });
    const { left, right } = splitPanes(model.split(), UNFOLD_STEP, {
      headers: [hunks[0].header, hunks[1].header],
      discard: true,
    });
    expect(left.hunkStarts).toEqual(right.hunkStarts);
    expect(left.hunkStarts).toHaveLength(2);
    expect(left.hunkOf).toEqual(right.hunkOf);
    // Only the change's (right) side gets the discard button, per hunk.
    expect(left.decorations.line(left.hunkStarts[0])?.actions).toEqual([]);
    expect(right.decorations.line(right.hunkStarts[0])?.actions).toHaveLength(1);
    expect(right.decorations.line(right.hunkStarts[1])?.actions).toHaveLength(1);
  });
});

describe("DiffHighlight", () => {
  it("asks each side for its own lines and maps the colours back", () => {
    const calls: string[] = [];
    const side = (name: string) => ({
      spans(first: number, last: number) {
        calls.push(`${name}:${first}-${last}`);
        const out = new Map();
        for (let l = first; l <= last; l++) out.set(l, [{ from: 0, to: 1, token: "keyword" as const }]);
        return out;
      },
    });
    const sources = [null, { side: "old" as const, line: 4 }, { side: "new" as const, line: 4 }, { side: "new" as const, line: 5 }];
    const spans = new DiffHighlight(sources, { old: side("old"), new: side("new") }).spans(0, 10);
    expect(calls).toEqual(["old:4-4", "new:4-5"]);
    expect([...spans.keys()]).toEqual([1, 2, 3]);
  });

  it("never merges a run across a fold row, even when the same side continues", () => {
    const calls: string[] = [];
    const side = (name: string) => ({
      spans(first: number, last: number) {
        calls.push(`${name}:${first}-${last}`);
        const out = new Map();
        for (let l = first; l <= last; l++) out.set(l, []);
        return out;
      },
    });
    // A fold hiding a large gap: the old side resumes far past where it left
    // off, exactly as `DiffModel`'s gap rows produce it.
    const sources = [
      { side: "old" as const, line: 4 },
      { side: "old" as const, line: 5 },
      null, // the fold row
      { side: "old" as const, line: 50 },
      { side: "old" as const, line: 51 },
    ];
    const spans = new DiffHighlight(sources, { old: side("old"), new: null }).spans(0, 10);
    expect(calls).toEqual(["old:4-5", "old:50-51"]);
    expect([...spans.keys()]).toEqual([0, 1, 3, 4]);
  });

  it("still splits into two runs across a fold even when the line numbers would otherwise be adjacent", () => {
    const calls: string[] = [];
    const side = (name: string) => ({
      spans(first: number, last: number) {
        calls.push(`${name}:${first}-${last}`);
        return new Map();
      },
    });
    const sources = [{ side: "old" as const, line: 4 }, null, { side: "old" as const, line: 5 }];
    new DiffHighlight(sources, { old: side("old"), new: null }).spans(0, 2);
    expect(calls).toEqual(["old:4-4", "old:5-5"]);
  });
});
