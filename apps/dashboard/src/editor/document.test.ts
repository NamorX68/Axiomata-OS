import { describe, expect, it } from "vitest";

import { run } from "./commands";
import { EditorDocument } from "./document";
import { cursor, pos, range } from "./position";
import { ctx, docFrom, show } from "./testing";

function type(doc: EditorDocument, text: string, now: number): void {
  for (const ch of text) run(doc, { type: "insert", text: ch }, ctx({ now }));
}

describe("undo grouping (F4)", () => {
  it("merges a run of typing into one step", () => {
    const doc = docFrom("|");
    type(doc, "hello", 0);
    expect(show(doc)).toBe("hello|");
    doc.undo();
    expect(show(doc)).toBe("|");
    doc.redo();
    expect(show(doc)).toBe("hello|");
  });

  it("starts a new step after a pause of more than a second", () => {
    const doc = docFrom("|");
    type(doc, "ab", 0);
    type(doc, "cd", 1500);
    doc.undo();
    expect(show(doc)).toBe("ab|");
  });

  it("starts a new step after a cursor jump", () => {
    const doc = docFrom("|xyz");
    type(doc, "ab", 0);
    doc.setSelection(cursor(pos(0, 5)));
    type(doc, "c", 10);
    doc.undo();
    expect(show(doc)).toBe("abxyz|");
    doc.undo();
    expect(show(doc)).toBe("|xyz");
  });

  it("keeps typing and deleting apart", () => {
    const doc = docFrom("|");
    type(doc, "abc", 0);
    run(doc, { type: "deleteBackward", unit: "char" }, ctx({ now: 10 }));
    run(doc, { type: "deleteBackward", unit: "char" }, ctx({ now: 20 }));
    expect(show(doc)).toBe("a|");
    doc.undo();
    expect(show(doc)).toBe("abc|");
    doc.undo();
    expect(show(doc)).toBe("|");
  });

  it("never merges a paste", () => {
    const doc = docFrom("|");
    type(doc, "a", 0);
    run(doc, { type: "insert", text: "PASTE", kind: "other" }, ctx({ now: 1 }));
    type(doc, "b", 2);
    doc.undo();
    expect(show(doc)).toBe("aPASTE|");
    doc.undo();
    expect(show(doc)).toBe("a|");
  });

  it("restores the selection a step started with", () => {
    const doc = docFrom("one ^two| three");
    run(doc, { type: "insert", text: "2" }, ctx());
    expect(show(doc)).toBe("one 2| three");
    doc.undo();
    expect(show(doc)).toBe("one ^two| three");
  });

  it("undoes and redoes multi-line changes exactly", () => {
    const doc = docFrom("a|b");
    run(doc, { type: "insert", text: "1\r\n2\n3", kind: "other" }, ctx());
    expect(show(doc)).toBe("a1\n2\n3|b");
    doc.undo();
    expect(show(doc)).toBe("a|b");
    doc.redo();
    expect(show(doc)).toBe("a1\n2\n3|b");
  });

  it("drops the redo history on a new edit", () => {
    const doc = docFrom("|");
    type(doc, "a", 0);
    doc.undo();
    type(doc, "b", 10);
    expect(doc.redo()).toBe(false);
    expect(show(doc)).toBe("b|");
  });
});

describe("dirty tracking", () => {
  it("is clean when opened, dirty after an edit, clean after save", () => {
    const doc = docFrom("|x");
    expect(doc.dirty).toBe(false);
    type(doc, "a", 0);
    expect(doc.dirty).toBe(true);
    doc.markSaved();
    expect(doc.dirty).toBe(false);
  });

  it("is clean again after undoing back to the save point", () => {
    const doc = docFrom("|");
    type(doc, "a", 0);
    doc.markSaved();
    type(doc, "b", 10);
    expect(doc.dirty).toBe(true);
    doc.undo();
    expect(doc.dirty).toBe(false);
    doc.redo();
    expect(doc.dirty).toBe(true);
  });

  it("does not merge typing into the step that was saved", () => {
    const doc = docFrom("|");
    type(doc, "a", 0);
    doc.markSaved();
    type(doc, "b", 10);
    doc.undo();
    expect(show(doc)).toBe("a|");
  });

  it("stays dirty after undoing past the save point and typing something else", () => {
    const doc = docFrom("|");
    type(doc, "a", 0);
    doc.markSaved();
    doc.undo();
    type(doc, "b", 2000);
    expect(doc.dirty).toBe(true);
  });
});

describe("saving and reloading", () => {
  it("saves in the file's own line endings with its final newline", () => {
    const doc = new EditorDocument("a\r\nb\r\n", { indentFallback: { kind: "spaces", size: 4 } });
    expect(doc.store.lineCount()).toBe(2);
    run(doc, { type: "move", motion: "docEnd", extend: false }, ctx());
    run(doc, { type: "newline" }, ctx());
    expect(doc.textForSave()).toBe("a\r\nb\r\n\r\n");
  });

  it("reloads as clean with no history, keeping the cursor where it can", () => {
    const doc = docFrom("line one\nline |two");
    type(doc, "x", 0);
    doc.reset("short\n");
    expect(doc.dirty).toBe(false);
    expect(doc.canUndo).toBe(false);
    expect(show(doc)).toBe("short|");
  });

  it("uses the file's indentation, or the fallback when it shows none", () => {
    expect(new EditorDocument("a\n\tb", { indentFallback: { kind: "spaces", size: 4 } }).indent).toEqual({
      kind: "tabs",
      size: 4,
    });
    const prose = new EditorDocument("prose", { indentFallback: { kind: "spaces", size: 2 } });
    expect(prose.indent).toEqual({ kind: "spaces", size: 2 });
    expect(prose.indentDetected).toBe(false);
  });
});

describe("undo groups (ED3, V5)", () => {
  it("joins every edit until the group ends into one step, kinds and pauses regardless", () => {
    const doc = new EditorDocument("abc", { indentFallback: { kind: "spaces", size: 4 } });
    doc.beginUndoGroup();
    doc.edit([{ range: range(pos(0, 0), pos(0, 1)), text: "" }], cursor(pos(0, 0)), "other", 0);
    doc.edit([{ range: range(pos(0, 0), pos(0, 0)), text: "X" }], cursor(pos(0, 1)), "typing", 5000);
    doc.edit([{ range: range(pos(0, 1), pos(0, 1)), text: "Y" }], cursor(pos(0, 2)), "deleting", 9000);
    doc.endUndoGroup();
    expect(doc.store.text()).toBe("XYbc");
    doc.undo();
    expect(doc.store.text()).toBe("abc");
    expect(doc.canUndo).toBe(false);
  });

  it("starts a new step after the group", () => {
    const doc = new EditorDocument("", { indentFallback: { kind: "spaces", size: 4 } });
    doc.beginUndoGroup();
    doc.edit([{ range: range(pos(0, 0), pos(0, 0)), text: "a" }], cursor(pos(0, 1)), "typing", 0);
    doc.endUndoGroup();
    doc.edit([{ range: range(pos(0, 1), pos(0, 1)), text: "b" }], cursor(pos(0, 2)), "typing", 1);
    doc.undo();
    expect(doc.store.text()).toBe("a");
  });
});
