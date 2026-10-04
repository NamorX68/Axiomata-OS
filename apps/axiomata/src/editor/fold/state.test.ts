import { describe, expect, it } from "vitest";

import { LineStore } from "../buffer";
import { run } from "../commands";
import { pos, range } from "../position";
import { ctx, docFrom, show } from "../testing";
import { VisualLayout } from "../visual";
import { FoldState } from "./state";

/** Ten numbered lines, with the cursor on the first. */
function tenLines() {
  return docFrom(`|${Array.from({ length: 10 }, (_, i) => `line ${i}`).join("\n")}`);
}

describe("FoldState", () => {
  it("hides the lines under each closed fold, nested ones merged into their outer", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 3 });
    folds.close({ start: 2, end: 3 });
    folds.close({ start: 6, end: 8 });
    expect(folds.hidden()).toEqual([
      { from: 2, to: 3 },
      { from: 7, to: 8 },
    ]);
    expect(folds.hiddenAt(3)).toEqual({ from: 2, to: 3 });
    expect(folds.hiddenAt(4)).toBeNull();
  });

  it("names the outermost closed fold a line belongs to", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 5 });
    folds.close({ start: 2, end: 3 });
    expect(folds.closedAround(3)).toEqual({ start: 1, end: 5 });
    expect(folds.closedAround(1)).toEqual({ start: 1, end: 5 });
    expect(folds.closedAround(6)).toBeNull();
    // Opening the outer one shows the inner still closed.
    folds.open(1);
    expect(folds.closedAround(3)).toEqual({ start: 2, end: 3 });
  });

  it("finds the nearest shown line either way", () => {
    const folds = new FoldState();
    folds.close({ start: 2, end: 4 });
    expect(folds.visibleLine(3, -1, 10)).toBe(2);
    expect(folds.visibleLine(3, 1, 10)).toBe(5);
    expect(folds.visibleLine(3, 1, 5)).toBeNull();
    expect(folds.visibleLine(7, 1, 10)).toBe(7);
  });

  it("counts the folded lines between two lines", () => {
    const folds = new FoldState();
    folds.close({ start: 2, end: 4 });
    folds.close({ start: 6, end: 9 });
    expect(folds.hiddenBetween(0, 5)).toBe(2);
    expect(folds.hiddenBetween(10, 1)).toBe(5);
  });

  it("reveal opens every fold that hides a line, not one whose header it is", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 5 });
    folds.close({ start: 2, end: 3 });
    expect(folds.reveal(1)).toBe(false);
    expect(folds.reveal(3)).toBe(true);
    expect(folds.closed).toEqual([]);
  });

  it("serializes, and restores only what fits the text", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 3 });
    const saved = folds.serialize();
    expect(saved).toEqual([[1, 3]]);
    const back = new FoldState();
    back.restore([...saved, [4, 20], [5, 5], ["x", 1], [1, 2]], 10);
    expect(back.closed).toEqual([{ start: 1, end: 3 }]);
  });
});

describe("FoldState following the text", () => {
  function folded() {
    const doc = tenLines();
    const folds = new FoldState(doc);
    folds.close({ start: 3, end: 6 });
    return { doc, folds };
  }

  it("moves with lines added or taken away above it", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(0, 0), pos(0, 0)), text: "new\n" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 4, end: 7 }]);
    // Right above the header: a whole line out (the line before, with its break).
    doc.edit([{ range: range(pos(3, 0), pos(4, 0)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 3, end: 6 }]);
  });

  it("stays through typing on its header line and edits below it", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(3, 2), pos(3, 2)), text: "xx" }], doc.selection);
    doc.edit([{ range: range(pos(8, 0), pos(9, 0)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 3, end: 6 }]);
  });

  it("stretches and shrinks with edits among its hidden lines", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(5, 0), pos(5, 0)), text: "a\nb\n" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 3, end: 8 }]);
    doc.edit([{ range: range(pos(4, 0), pos(7, 0)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 3, end: 5 }]);
  });

  it("opens when an edit reaches across its header or its last line", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(3, 6), pos(3, 6)), text: "\n" }], doc.selection);
    expect(folds.closed).toEqual([]);
    folds.close({ start: 3, end: 6 });
    doc.edit([{ range: range(pos(6, 2), pos(7, 2)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([]);
  });

  it("goes with the lines when they are deleted whole", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(3, 0), pos(7, 0)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([]);
  });
});

describe("VisualLayout with folds", () => {
  it("gives folded lines no rows and never maps a row to one", () => {
    const store = new LineStore(Array.from({ length: 6 }, (_, i) => `l${i}`).join("\n"));
    const layout = new VisualLayout(store, { wrap: false, width: 80, tabSize: 4 });
    const folds = new FoldState();
    folds.close({ start: 1, end: 3 });
    layout.setFolds(folds);
    // Six lines, two of them folded away.
    expect(layout.totalRows).toBe(4);
    expect(layout.lineAt(1)).toEqual({ line: 1, sub: 0 });
    expect(layout.lineAt(2)).toEqual({ line: 4, sub: 0 });
    expect(layout.firstRow(4)).toBe(2);
    expect(layout.visibleLine(2, 1)).toBe(4);
  });

  it("keeps a fold that reaches the end of the text off the last row", () => {
    const store = new LineStore("a\nb\nc");
    const layout = new VisualLayout(store, { wrap: false, width: 80, tabSize: 4 });
    const folds = new FoldState();
    folds.close({ start: 0, end: 2 });
    layout.setFolds(folds);
    expect(layout.totalRows).toBe(1);
    expect(layout.lineAt(5)).toEqual({ line: 0, sub: 0 });
  });
});

describe("moving over folds (normal key map)", () => {
  function setup(marked: string, fold: { start: number; end: number }) {
    const doc = docFrom(marked);
    const layout = new VisualLayout(doc.store, { wrap: false, width: 80, tabSize: 4 });
    const folds = new FoldState(doc);
    folds.close(fold);
    layout.setFolds(folds);
    const move = (motion: "up" | "down" | "charLeft" | "charRight" | "docEnd") =>
      run(doc, { type: "move", motion, extend: false }, ctx({ layout }));
    return { doc, move };
  }

  it("↓ and ↑ step over a fold as over one line", () => {
    const { doc, move } = setup("a|a\nbb\ncc\ndd\nee", { start: 1, end: 3 });
    move("down");
    expect(show(doc)).toBe("aa\nb|b\ncc\ndd\nee");
    move("down");
    expect(show(doc)).toBe("aa\nbb\ncc\ndd\ne|e");
    move("up");
    expect(show(doc)).toBe("aa\nb|b\ncc\ndd\nee");
  });

  it("→ at a folded header's end goes past the fold, ← comes back to it", () => {
    const { doc, move } = setup("aa\nbb|\ncc\ndd", { start: 1, end: 2 });
    move("charRight");
    expect(show(doc)).toBe("aa\nbb\ncc\n|dd");
    move("charLeft");
    expect(show(doc)).toBe("aa\nbb|\ncc\ndd");
  });

  it("⌥→ and ⌥← step over a fold like → and ←", () => {
    const doc = docFrom("aa\nbb|\ncc\ndd");
    const layout = new VisualLayout(doc.store, { wrap: false, width: 80, tabSize: 4 });
    const folds = new FoldState(doc);
    folds.close({ start: 1, end: 2 });
    layout.setFolds(folds);
    run(doc, { type: "move", motion: "wordRight", extend: false }, ctx({ layout }));
    expect(show(doc)).toBe("aa\nbb\ncc\n|dd");
    run(doc, { type: "move", motion: "wordLeft", extend: false }, ctx({ layout }));
    expect(show(doc)).toBe("aa\nbb|\ncc\ndd");
  });

  it("→ at a folded header that ends the text stays put", () => {
    const { doc, move } = setup("aa\nbb|\ncc", { start: 1, end: 2 });
    move("charRight");
    expect(show(doc)).toBe("aa\nbb|\ncc");
  });

  it("↑ on the first row under a fold at the top of the text lands on the header", () => {
    const { doc, move } = setup("aa\nbb\ncc\n|dd", { start: 0, end: 2 });
    move("up");
    expect(show(doc)).toBe("|aa\nbb\ncc\ndd");
  });

  it("↓ on the last row and ⌘↓ stop at the header of a fold that ends the text", () => {
    const { doc, move } = setup("a|a\nbb\ncc", { start: 0, end: 2 });
    move("down");
    expect(show(doc)).toBe("aa|\nbb\ncc");
    doc.setSelection({ anchor: pos(0, 0), head: pos(0, 0) });
    move("docEnd");
    expect(show(doc)).toBe("aa|\nbb\ncc");
  });
});

describe("FoldState edge cases", () => {
  it("ignores a fold of one line, and bumps its version only when what is folded changes", () => {
    const folds = new FoldState();
    const v0 = folds.version;
    folds.close({ start: 2, end: 2 });
    expect(folds.closed).toEqual([]);
    expect(folds.version).toBe(v0);
    folds.openAll();
    expect(folds.version).toBe(v0);
    expect(folds.open(4)).toBe(false);
    expect(folds.version).toBe(v0);
    folds.close({ start: 2, end: 4 });
    expect(folds.version).toBe(v0 + 1);
  });

  it("replaces a closed fold with the same header on close", () => {
    const folds = new FoldState();
    folds.close({ start: 2, end: 4 });
    folds.close({ start: 2, end: 7 });
    expect(folds.closed).toEqual([{ start: 2, end: 7 }]);
  });

  it("closeAll keeps folds already closed, adds new ones in order, and skips one-line ranges", () => {
    const folds = new FoldState();
    folds.close({ start: 5, end: 6 });
    folds.closeAll([
      { start: 8, end: 9 },
      { start: 1, end: 3 },
      { start: 4, end: 4 },
    ]);
    expect(folds.closed).toEqual([
      { start: 1, end: 3 },
      { start: 5, end: 6 },
      { start: 8, end: 9 },
    ]);
  });

  it("keeps two folds side by side apart: the second header stays shown", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 3 });
    folds.close({ start: 4, end: 6 });
    expect(folds.hidden()).toEqual([
      { from: 2, to: 3 },
      { from: 5, to: 6 },
    ]);
    expect(folds.hiddenAt(4)).toBeNull();
    expect(folds.closedAround(3)).toEqual({ start: 1, end: 3 });
    expect(folds.closedAround(4)).toEqual({ start: 4, end: 6 });
    expect(folds.hiddenBetween(0, 7)).toBe(4);
  });

  it("an inner fold that reaches past its outer one widens what the outer hides", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 4 });
    folds.close({ start: 3, end: 8 });
    expect(folds.hidden()).toEqual([{ from: 2, to: 8 }]);
    expect(folds.closedAround(7)).toEqual({ start: 1, end: 8 });
  });

  it("reveal of a line in the outer fold only keeps an inner fold that does not hide it", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 8 });
    folds.close({ start: 3, end: 5 });
    expect(folds.reveal(7)).toBe(true);
    expect(folds.closed).toEqual([{ start: 3, end: 5 }]);
    expect(folds.hiddenAt(4)).toEqual({ from: 4, to: 5 });
    expect(folds.hiddenAt(7)).toBeNull();
    // A line nested in both opens both.
    folds.close({ start: 1, end: 8 });
    expect(folds.reveal(4)).toBe(true);
    expect(folds.closed).toEqual([]);
  });

  it("finds shown lines at the edges of the text", () => {
    const folds = new FoldState();
    folds.close({ start: 0, end: 9 });
    // The header on line 0 is shown; everything under it, to the last line, is not.
    expect(folds.visibleLine(0, 1, 10)).toBe(0);
    expect(folds.visibleLine(0, -1, 10)).toBe(0);
    expect(folds.visibleLine(9, -1, 10)).toBe(0);
    expect(folds.visibleLine(9, 1, 10)).toBeNull();
    expect(folds.visibleLine(1, 1, 10)).toBeNull();
    // One line more, and going down lands on it.
    expect(folds.visibleLine(1, 1, 11)).toBe(10);
  });

  it("closedAround is null on a header whose fold is open, and on lines outside any fold", () => {
    const folds = new FoldState();
    folds.close({ start: 2, end: 4 });
    folds.open(2);
    expect(folds.closedAround(2)).toBeNull();
    expect(folds.closedAround(3)).toBeNull();
  });

  it("restores from anything that is not a list as no folds", () => {
    const folds = new FoldState();
    folds.close({ start: 1, end: 2 });
    folds.restore({ 1: 2 }, 10);
    expect(folds.closed).toEqual([]);
    folds.restore([[3, 5], [-1, 2], [1, 2, 3], [1.5, 3], [2, 4]], 10);
    expect(folds.closed).toEqual([
      { start: 2, end: 4 },
      { start: 3, end: 5 },
    ]);
  });

  it("stops following the text once disposed", () => {
    const doc = tenLines();
    const folds = new FoldState(doc);
    folds.close({ start: 3, end: 6 });
    folds.dispose();
    doc.edit([{ range: range(pos(0, 0), pos(0, 0)), text: "new\n" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 3, end: 6 }]);
  });
});

describe("FoldState following the text, edge cases", () => {
  function folded() {
    const doc = tenLines();
    const folds = new FoldState(doc);
    folds.close({ start: 3, end: 6 });
    return { doc, folds };
  }

  it("moves back when the edit that moved it is undone, and again on redo", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(1, 0), pos(1, 0)), text: "x\ny\n" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 5, end: 8 }]);
    doc.undo();
    expect(folds.closed).toEqual([{ start: 3, end: 6 }]);
    doc.redo();
    expect(folds.closed).toEqual([{ start: 5, end: 8 }]);
  });

  it("shrinks back when a stretch among its hidden lines is undone", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(5, 0), pos(5, 0)), text: "a\nb\n" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 3, end: 8 }]);
    doc.undo();
    expect(folds.closed).toEqual([{ start: 3, end: 6 }]);
  });

  it("moves down with whole lines put in at its header's first column (O on the header)", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(3, 0), pos(3, 0)), text: "above\n" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 4, end: 7 }]);
    expect(doc.store.line(4)).toBe("line 3");
  });

  it("opens when its header line is deleted whole", () => {
    const { doc, folds } = folded();
    const before = folds.version;
    doc.edit([{ range: range(pos(3, 0), pos(4, 0)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([]);
    expect(folds.version).toBe(before + 1);
  });

  it("does not bump its version when the text only shifts it", () => {
    const { doc, folds } = folded();
    const before = folds.version;
    doc.edit([{ range: range(pos(0, 0), pos(0, 0)), text: "new\n" }], doc.selection);
    expect(folds.version).toBe(before);
  });

  it("stays through an edit within its last hidden line", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(6, 0), pos(6, 4)), text: "LINE" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 3, end: 6 }]);
  });

  it("opens when its hidden lines are all taken away", () => {
    const { doc, folds } = folded();
    doc.edit([{ range: range(pos(4, 0), pos(7, 0)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([]);
  });

  it("stretches nested folds together, and moves a later one", () => {
    const doc = tenLines();
    const folds = new FoldState(doc);
    folds.close({ start: 1, end: 6 });
    folds.close({ start: 2, end: 4 });
    folds.close({ start: 7, end: 9 });
    doc.edit([{ range: range(pos(3, 0), pos(3, 0)), text: "x\n" }], doc.selection);
    expect(folds.closed).toEqual([
      { start: 1, end: 7 },
      { start: 2, end: 5 },
      { start: 8, end: 10 },
    ]);
  });

  it("keeps the outer fold when an edit opens only the inner one", () => {
    const doc = tenLines();
    const folds = new FoldState(doc);
    folds.close({ start: 1, end: 6 });
    folds.close({ start: 2, end: 4 });
    // Joins the inner fold's last line with the line after it: across its end, inside the outer.
    doc.edit([{ range: range(pos(4, 6), pos(5, 0)), text: "" }], doc.selection);
    expect(folds.closed).toEqual([{ start: 1, end: 5 }]);
  });

  it("the layout follows once refreshed", () => {
    const doc = tenLines();
    const layout = new VisualLayout(doc.store, { wrap: false, width: 80, tabSize: 4 });
    const folds = new FoldState(doc);
    folds.close({ start: 3, end: 6 });
    layout.setFolds(folds);
    expect(layout.totalRows).toBe(7);
    doc.edit([{ range: range(pos(5, 0), pos(5, 0)), text: "a\nb\n" }], doc.selection);
    layout.refresh();
    expect(layout.totalRows).toBe(7);
    expect(layout.lineAt(4)).toEqual({ line: 9, sub: 0 });
  });
});

describe("VisualLayout with wrapping and folds", () => {
  // "aaaa bbbb cccc" wraps into three rows at width 8, "x" is one.
  const TEXT = ["aaaa bbbb cccc", "aaaa bbbb cccc", "aaaa bbbb cccc", "x", "aaaa bbbb cccc"].join("\n");

  it("keeps the header's own rows and none of the hidden lines'", () => {
    const layout = new VisualLayout(new LineStore(TEXT), { wrap: true, width: 8, tabSize: 4 });
    expect(layout.totalRows).toBe(3 + 3 + 3 + 1 + 3);
    const folds = new FoldState();
    folds.close({ start: 0, end: 2 });
    layout.setFolds(folds);
    expect(layout.totalRows).toBe(3 + 1 + 3);
    expect(layout.lineAt(2)).toEqual({ line: 0, sub: 2 });
    expect(layout.lineAt(3)).toEqual({ line: 3, sub: 0 });
    expect(layout.lineAt(5)).toEqual({ line: 4, sub: 1 });
    expect(layout.firstRow(3)).toBe(3);
    expect(layout.firstRow(4)).toBe(4);
    // Rows of a wrapped header are still asked for line by line.
    expect(layout.rowStarts(0)).toEqual([0, 5, 10]);
  });

  it("keeps rows right after setOptions and when the folds are taken away", () => {
    const layout = new VisualLayout(new LineStore(TEXT), { wrap: true, width: 8, tabSize: 4 });
    const folds = new FoldState();
    folds.close({ start: 1, end: 3 });
    layout.setFolds(folds);
    expect(layout.totalRows).toBe(3 + 3 + 3);
    layout.setOptions({ wrap: false, width: 8, tabSize: 4 });
    expect(layout.totalRows).toBe(3);
    expect(layout.lineAt(2)).toEqual({ line: 4, sub: 0 });
    layout.setFolds(null);
    expect(layout.totalRows).toBe(5);
    expect(layout.visibleLine(2, 1)).toBe(2);
  });

  it("↑ from the line under a fold lands on the last row of a wrapped header, keeping the column", () => {
    const doc = docFrom(["aaaa bbbb cccc", "hidden", "hidden", "xx|xxxx"].join("\n"));
    const layout = new VisualLayout(doc.store, { wrap: true, width: 8, tabSize: 4 });
    const folds = new FoldState(doc);
    folds.close({ start: 0, end: 2 });
    layout.setFolds(folds);
    run(doc, { type: "move", motion: "up", extend: false }, ctx({ layout }));
    expect(show(doc)).toBe(["aaaa bbbb cc|cc", "hidden", "hidden", "xxxxxx"].join("\n"));
    run(doc, { type: "move", motion: "down", extend: false }, ctx({ layout }));
    expect(show(doc)).toBe(["aaaa bbbb cccc", "hidden", "hidden", "xx|xxxx"].join("\n"));
  });

  it("↓ from the last row of a wrapped header goes past the fold", () => {
    const doc = docFrom(["aaaa bbbb c|ccc", "hidden", "hidden", "xxxxxx"].join("\n"));
    const layout = new VisualLayout(doc.store, { wrap: true, width: 8, tabSize: 4 });
    const folds = new FoldState(doc);
    folds.close({ start: 0, end: 2 });
    layout.setFolds(folds);
    run(doc, { type: "move", motion: "down", extend: false }, ctx({ layout }));
    expect(show(doc)).toBe(["aaaa bbbb cccc", "hidden", "hidden", "x|xxxxx"].join("\n"));
  });
});
