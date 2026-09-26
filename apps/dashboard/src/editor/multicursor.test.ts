import { describe, expect, it } from "vitest";

import { copyText, cut, paste, run, type ClipboardText, type Command } from "./commands";
import { EditorDocument } from "./document";
import {
  addCursorVertical,
  addNextOccurrence,
  allSelections,
  columnSelection,
  removeLastCursor,
  selectAllOccurrences,
  singleCursor,
  toggleCursor,
} from "./multicursor";
import { comparePos, cursor, pos, type Pos, type Selection } from "./position";
import { ctx } from "./testing";

/**
 * Several cursors in marked text: every `|` is a cursor head; a `^` before it
 * is its anchor. The last cursor in the text is the main one unless `main`
 * says which (0-based, in text order).
 */
function docWith(marked: string, main?: number): EditorDocument {
  const sels: Selection[] = [];
  let anchor: Pos | null = null;
  let line = 0;
  let col = 0;
  let text = "";
  for (const ch of marked) {
    if (ch === "^") anchor = pos(line, col);
    else if (ch === "|") {
      sels.push({ anchor: anchor ?? pos(line, col), head: pos(line, col) });
      anchor = null;
    } else {
      text += ch;
      if (ch === "\n") {
        line++;
        col = 0;
      } else col += ch.length;
    }
  }
  const doc = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } as never });
  const m = main ?? sels.length - 1;
  doc.setSelections(sels[m], sels.filter((_, i) => i !== m));
  return doc;
}

/** Every cursor put back into the text, `|` heads and `^` anchors. */
function showAll(doc: EditorDocument): string {
  const lines = doc.store.text().split("\n");
  const marks: Array<{ at: Pos; mark: string }> = [];
  for (const s of allSelections(doc)) {
    marks.push({ at: s.head, mark: "|" });
    if (comparePos(s.anchor, s.head) !== 0) marks.push({ at: s.anchor, mark: "^" });
  }
  marks.sort((a, b) => comparePos(b.at, a.at) || (a.mark === "|" ? -1 : 1));
  for (const { at, mark } of marks) lines[at.line] = lines[at.line].slice(0, at.col) + mark + lines[at.line].slice(at.col);
  return lines.join("\n");
}

function runAll(marked: string, cmds: Command[]): string {
  const doc = docWith(marked);
  for (const cmd of cmds) run(doc, cmd, ctx());
  return showAll(doc);
}

describe("a command at every cursor (T6)", () => {
  it.each<[string, Command[], string]>([
    ["a|b\nc|d", [{ type: "insert", text: "X" }], "aX|b\ncX|d"],
    ["a|b a|b", [{ type: "insert", text: "XY" }], "aXY|b aXY|b"],
    ["ab|\ncd|", [{ type: "deleteBackward", unit: "char" }], "a|\nc|"],
    ["|ab\n|cd", [{ type: "move", motion: "lineEnd", extend: false }], "ab|\ncd|"],
    ["|ab\n|cd", [{ type: "move", motion: "lineEnd", extend: true }], "^ab|\n^cd|"],
    ["a|b\nc|d", [{ type: "newline" }], "a\n|b\nc\n|d"],
    // Two cursors that meet become one.
    ["a|b|c", [{ type: "deleteBackward", unit: "char" }], "|c"],
    ["x^ab|x ^ab|x", [{ type: "insert", text: "Q" }], "xQ|x Q|x"],
    // A whole-line command takes a line once, however many cursors are on it.
    ["    a|b|\n    c|d", [{ type: "outdent" }], "a|b|\nc|d"],
    // ⇥ at bare cursors puts indentation at each, to the next tab stop.
    ["a|b|\nc|d", [{ type: "indent" }], "a   |b  |\nc   |d"],
  ])("%j %j", (before, cmds, after) => {
    expect(runAll(before, cmds)).toBe(after);
  });

  it("undoes the edit at every cursor at once, and puts them all back", () => {
    const doc = docWith("a|b\nc|d");
    run(doc, { type: "insert", text: "X" }, ctx({ now: 0 }));
    run(doc, { type: "insert", text: "Y" }, ctx({ now: 10 }));
    expect(showAll(doc)).toBe("aXY|b\ncXY|d");
    // Typing with several cursors stays one step, as with one.
    run(doc, { type: "undo" }, ctx());
    expect(showAll(doc)).toBe("a|b\nc|d");
    run(doc, { type: "redo" }, ctx());
    expect(showAll(doc)).toBe("aXY|b\ncXY|d");
  });
});

describe("getting more cursors (T6)", () => {
  it("⌥-click adds a cursor, and on a cursor takes it away", () => {
    const doc = docWith("|abc");
    toggleCursor(doc, pos(0, 2));
    expect(showAll(doc)).toBe("|ab|c");
    toggleCursor(doc, pos(0, 2));
    expect(showAll(doc)).toBe("|abc");
  });

  it("⌥⌘↓ adds one below at the same column, past a short line's end", () => {
    const doc = docWith("ab|cd\nx\nabcd");
    addCursorVertical(doc, 1, 4);
    expect(showAll(doc)).toBe("ab|cd\nx|\nabcd");
    addCursorVertical(doc, 1, 4);
    expect(showAll(doc)).toBe("ab|cd\nx|\nab|cd");
    addCursorVertical(doc, -1, 4);
    expect(showAll(doc)).toBe("ab|cd\nx|\nab|cd");
  });

  it("⌘D takes the word, then its next occurrences, wrapping around; ⌘U takes the last back", () => {
    const doc = docWith("foo x f|oo y foo");
    addNextOccurrence(doc);
    expect(showAll(doc)).toBe("foo x ^foo| y foo");
    addNextOccurrence(doc);
    expect(showAll(doc)).toBe("foo x ^foo| y ^foo|");
    addNextOccurrence(doc);
    expect(showAll(doc)).toBe("^foo| x ^foo| y ^foo|");
    // The newest is the main one.
    expect(doc.selection.anchor).toEqual(pos(0, 0));
    addNextOccurrence(doc);
    expect(allSelections(doc)).toHaveLength(3);
    removeLastCursor(doc);
    expect(showAll(doc)).toBe("foo x ^foo| y ^foo|");
  });

  it("⇧⌘L selects every occurrence of the word at the cursor", () => {
    const doc = docWith("ab x a|b y ab");
    selectAllOccurrences(doc);
    expect(showAll(doc)).toBe("^ab| x ^ab| y ^ab|");
    expect(doc.selection.anchor).toEqual(pos(0, 5));
  });

  it("Esc keeps only the main cursor", () => {
    const doc = docWith("|a|b|c", 1);
    expect(singleCursor(doc)).toBe(true);
    expect(showAll(doc)).toBe("a|bc");
    expect(singleCursor(doc)).toBe(false);
  });

  it("⌥-drag draws a column, a short line getting a cursor at its end", () => {
    const doc = docWith("|abcd\nx\nabcd");
    columnSelection(doc, pos(0, 1), pos(2, 3), 4);
    // Line 2 is too short for the column: its selection is empty, a bare cursor at its end.
    expect(showAll(doc)).toBe("a^bc|d\nx|\na^bc|d");
  });
});

describe("the clipboard with several cursors (T6)", () => {
  it("copies each selection, pastes one per cursor when the count matches", () => {
    const doc = docWith("^a|1 ^b|2");
    const clip = copyText(doc);
    expect(clip.text).toBe("a\nb");
    const target = docWith("x| y|");
    paste(target, clip, ctx());
    expect(showAll(target)).toBe("xa| yb|");
  });

  it("pastes the whole text at every cursor when the count does not match", () => {
    const target = docWith("x| y|");
    paste(target, { text: "q", wholeLine: false }, ctx());
    expect(showAll(target)).toBe("xq| yq|");
  });

  it("cuts every selection as one undo step", () => {
    const doc = docWith("^a|1 ^b|2");
    cut(doc, ctx());
    expect(showAll(doc)).toBe("|1 |2");
    run(doc, { type: "undo" }, ctx());
    expect(doc.store.text()).toBe("a1 b2");
  });

  it("cuts a whole line once, however many cursors sit on it", () => {
    // Both cursors are bare and share line 0: `copyText` sees one line to
    // copy, and `oncePerLine` must keep the cut from happening twice.
    const doc = docWith("a|b|\ncd");
    const clip = cut(doc, ctx());
    expect(clip.text).toBe("ab\n");
    expect(doc.store.text()).toBe("cd");
  });

  it("pastes by matching the text's own line count when the parts count differs", () => {
    // `parts` came from two cursors, but one of them held a two-line
    // selection: the joined text splits into three lines, matching the three
    // cursors it is now pasted into.
    const clip: ClipboardText = { text: "ab\ncd\nef", wholeLine: false, parts: ["ab\ncd", "ef"] };
    const target = docWith("x| y| z|");
    paste(target, clip, ctx());
    expect(showAll(target)).toBe("xab| ycd| zef|");
  });
});

describe("whole-line commands with several cursors (T6)", () => {
  it("moves adjacent-line cursors down as if the block moved together", () => {
    const doc = docWith("a|\nb|\nc");
    run(doc, { type: "moveLines", dir: 1 }, ctx());
    expect(doc.store.text()).toBe("c\na\nb");
  });

  // Found in the ED5.3 review: the second cursor's move was swallowed. Lines now move per block.
  it("moves adjacent-line cursors up as if the block moved together", () => {
    const doc = docWith("a\nb|\nc|");
    run(doc, { type: "moveLines", dir: -1 }, ctx());
    expect(doc.store.text()).toBe("b\nc\na");
    // The cursors go with their lines, and it is one undo step.
    expect(showAll(doc)).toBe("b|\nc|\na");
    run(doc, { type: "undo" }, ctx());
    expect(doc.store.text()).toBe("a\nb\nc");
  });

  it("moves two blocks separated by a line each on their own", () => {
    const doc = docWith("x\na|\ny\nb|");
    run(doc, { type: "moveLines", dir: -1 }, ctx());
    expect(showAll(doc)).toBe("a|\nx\nb|\ny");
  });

  it("duplicates each cursor's own line, even when they are adjacent", () => {
    const doc = docWith("one|\ntwo|");
    run(doc, { type: "duplicateLines", dir: 1 }, ctx());
    expect(doc.store.text()).toBe("one\none\ntwo\ntwo");
  });

  it("comments each cursor's own line, even when they are adjacent", () => {
    const doc = docWith("one|\ntwo|");
    run(doc, { type: "toggleComment" }, ctx());
    expect(doc.store.text()).toBe("// one\n// two");
  });

  it("outdents once for two cursors sharing a line, and again for a second line", () => {
    const doc = docWith("    a|b|\n        c|d");
    run(doc, { type: "outdent" }, ctx());
    expect(doc.store.text()).toBe("ab\n    cd");
  });
});

describe("deleting with several cursors (T6)", () => {
  it("deletes a word backward at each cursor", () => {
    const doc = docWith("foo bar|\nbaz qux|");
    run(doc, { type: "deleteBackward", unit: "word" }, ctx());
    expect(showAll(doc)).toBe("foo |\nbaz |");
  });

  it("deletes to the line start at each cursor, once per line", () => {
    const doc = docWith("foo b|ar|\nbaz qux|");
    run(doc, { type: "deleteBackward", unit: "line" }, ctx());
    expect(showAll(doc)).toBe("|\n|");
  });
});

describe("newline with several cursors keeps each one's own indentation (T6)", () => {
  it("carries each line's leading whitespace into its own new line", () => {
    const doc = docWith("    foo|\n        bar|");
    run(doc, { type: "newline" }, ctx());
    expect(showAll(doc)).toBe("    foo\n    |\n        bar\n        |");
  });
});

describe("a cursor jump seals the typing step, even with several cursors (T6)", () => {
  it("keeps typing before and after a move as separate undo steps", () => {
    const doc = docWith("a|b\nc|d");
    run(doc, { type: "insert", text: "X" }, ctx({ now: 0 }));
    run(doc, { type: "move", motion: "charLeft", extend: false }, ctx());
    run(doc, { type: "insert", text: "Y" }, ctx({ now: 5 }));
    const withBoth = doc.store.text();
    expect(withBoth).toContain("X");
    expect(withBoth).toContain("Y");
    // One undo takes back only the typing after the move, not the typing before it.
    run(doc, { type: "undo" }, ctx());
    const afterOneUndo = doc.store.text();
    expect(afterOneUndo).toContain("X");
    expect(afterOneUndo).not.toContain("Y");
    run(doc, { type: "undo" }, ctx());
    expect(doc.store.text()).toBe("ab\ncd");
  });
});

describe("undo/redo restores every cursor across a ⌘D and a keystroke (T6)", () => {
  it("puts every selection back the way ⌘D left it, before the typed character", () => {
    const doc = docWith("fo|o bar foo");
    addNextOccurrence(doc); // selects the word at the cursor
    addNextOccurrence(doc); // adds the next occurrence
    expect(showAll(doc)).toBe("^foo| bar ^foo|");
    run(doc, { type: "insert", text: "X" }, ctx({ now: 0 }));
    expect(showAll(doc)).toBe("X| bar X|");
    run(doc, { type: "undo" }, ctx());
    expect(showAll(doc)).toBe("^foo| bar ^foo|");
    run(doc, { type: "redo" }, ctx());
    expect(showAll(doc)).toBe("X| bar X|");
  });
});

describe("⌘D edge cases (T6)", () => {
  it("adds the next occurrence even when the selected text spans a line break", () => {
    const doc = docWith("^ab\ncd|\nab\ncd");
    addNextOccurrence(doc);
    expect(showAll(doc)).toBe("^ab\ncd|\n^ab\ncd|");
  });

  it("does nothing once every occurrence is already a cursor", () => {
    const doc = docWith("^foo| x ^foo|");
    addNextOccurrence(doc);
    expect(showAll(doc)).toBe("^foo| x ^foo|");
  });
});

describe("⌥⌘↑/↓ at the document's edge (T6)", () => {
  it("does nothing for ⌥⌘↑ already on the first line", () => {
    const doc = docWith("ab|cd\nx\nabcd");
    addCursorVertical(doc, -1, 4);
    expect(showAll(doc)).toBe("ab|cd\nx\nabcd");
  });
});

describe("column selection with tabs and drawn upward (T6)", () => {
  it("draws upward, keeping display columns through a tab", () => {
    // Column 1 on a tab-indented line is display column 4 (right after the
    // tab expands to the next stop); column 5 is display column 8, past the
    // whole word. Line 1 ("x") is too short for either column: a bare cursor
    // at its end, same as the existing downward-drag test.
    const doc = docWith("|\tabcd\nx\n\tabcd");
    columnSelection(doc, pos(2, 1), pos(0, 5), 4);
    expect(showAll(doc)).toBe("\t^abcd|\nx|\n\t^abcd|");
  });
});

describe("Vi interplay: a plain setSelection clears the extra cursors (T6)", () => {
  it("drops every extra cursor when something calls setSelection directly", () => {
    const doc = new EditorDocument("abc\ndef", { indentFallback: { kind: "spaces", size: 4 } as never });
    doc.setSelections(cursor(pos(0, 0)), [cursor(pos(1, 0))]);
    expect(doc.extra).toHaveLength(1);
    // A Vi machine moving the cursor calls the plain setSelection, as any
    // single-cursor caller would — it must not need to know about `extra`.
    doc.setSelection(cursor(pos(0, 1)));
    expect(doc.extra).toHaveLength(0);
  });
});

describe("each cursor's own goal column (ED5.3 review)", () => {
  it("keeps every cursor's column through ↓, also past a short line", () => {
    const doc = docWith("abcde|f ab|c\nx\nabcdefghijkl");
    run(doc, { type: "move", motion: "down", extend: false }, ctx());
    expect(showAll(doc)).toBe("abcdef abc\nx|\nabcdefghijkl");
    // Both landed at the short line's end and merged — typical: one cursor goes on.
    const two = docWith("ab|cd\nx\nabcd\nab|cd\nx\nabcd");
    run(two, { type: "move", motion: "down", extend: false }, ctx());
    run(two, { type: "move", motion: "down", extend: false }, ctx());
    expect(showAll(two)).toBe("abcd\nx\nab|cd\nabcd\nx\nab|cd");
  });
});

describe("⌥-drag next to earlier cursors (ED5.3 review)", () => {
  it("adds the column to the cursors that were there", () => {
    const doc = docWith("|abcd\nabcd\nabcd\nzz|z");
    const before = allSelections(doc);
    columnSelection(doc, pos(0, 1), pos(1, 2), 4, before);
    expect(showAll(doc)).toBe("|a^b|cd\na^b|cd\nabcd\nzz|z");
  });
});
