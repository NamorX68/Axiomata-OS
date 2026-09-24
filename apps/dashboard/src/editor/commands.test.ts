import { describe, expect, it } from "vitest";

import { commentPrefixFor, copyText, cut, paste, run, type Command, type Motion, type RowLayout } from "./commands";
import { ctx, docFrom, show } from "./testing";

/** Runs `cmd` on the marked-up `before` and returns the marked-up result. */
function after(before: string, cmd: Command, overrides = {}): string {
  const doc = docFrom(before);
  run(doc, cmd, ctx(overrides));
  return show(doc);
}

/** As `after`, with the indentation fixed at four spaces whatever the text suggests. */
function afterIndented(before: string, cmd: Command): string {
  const doc = docFrom(before);
  doc.indent = { kind: "spaces", size: 4 };
  run(doc, cmd, ctx());
  return show(doc);
}

function moved(before: string, motion: Motion, extend = false, overrides = {}): string {
  return after(before, { type: "move", motion, extend }, overrides);
}

describe("horizontal motion", () => {
  it("moves by grapheme and across line ends", () => {
    expect(moved("a|😀b", "charRight")).toBe("a😀|b");
    expect(moved("ab|\ncd", "charRight")).toBe("ab\n|cd");
    expect(moved("ab\n|cd", "charLeft")).toBe("ab|\ncd");
    expect(moved("|ab", "charLeft")).toBe("|ab");
  });

  it("collapses a selection to the side ←/→ point at", () => {
    expect(moved("a^bc|d", "charLeft")).toBe("a|bcd");
    expect(moved("a|bc^d", "charRight")).toBe("abc|d");
  });

  it("extends with ⇧", () => {
    expect(moved("a|bc", "charRight", true)).toBe("a^b|c");
    expect(moved("a^b|c", "charRight", true)).toBe("a^bc|");
  });

  it("jumps words, stopping at dots in code", () => {
    expect(moved("|let foo.bar", "wordRight")).toBe("let| foo.bar");
    expect(moved("let| foo.bar", "wordRight")).toBe("let foo|.bar");
    expect(moved("let foo.bar|", "wordLeft")).toBe("let foo.|bar");
    expect(moved("|x\nnext", "wordLeft")).toBe("|x\nnext");
    expect(moved("x|\nnext", "wordRight")).toBe("x\n|next");
  });

  it("⌘← goes to the first non-blank, then to column 0, and back", () => {
    expect(moved("    code|", "lineStart")).toBe("    |code");
    expect(moved("    |code", "lineStart")).toBe("|    code");
    expect(moved("|    code", "lineStart")).toBe("    |code");
    expect(moved("    |code", "lineEnd")).toBe("    code|");
  });

  it("goes to the start and end of the document", () => {
    expect(moved("a\nb|c\nd", "docStart")).toBe("|a\nbc\nd");
    expect(moved("a\nb|c\nd", "docEnd")).toBe("a\nbc\nd|");
  });
});

describe("vertical motion", () => {
  it("keeps the goal column across a short line", () => {
    const doc = docFrom("long line|\nab\nanother line");
    run(doc, { type: "move", motion: "down", extend: false }, ctx());
    expect(show(doc)).toBe("long line\nab|\nanother line");
    run(doc, { type: "move", motion: "down", extend: false }, ctx());
    expect(show(doc)).toBe("long line\nab\nanother l|ine");
  });

  it("goes to the very start or end past the first or last line", () => {
    expect(moved("ab|c\nd", "up")).toBe("|abc\nd");
    expect(moved("a\nd|ef", "down")).toBe("a\ndef|");
  });

  it("counts a tab by its width", () => {
    expect(moved("\tx|\n12345678", "down")).toBe("\tx\n12345|678");
  });

  it("pages by the rows a page holds", () => {
    expect(moved("|0\n1\n2\n3\n4", "pageDown")).toBe("0\n1\n2\n|3\n4");
    expect(moved("0\n1\n2\n3\n|4", "pageUp")).toBe("0\n|1\n2\n3\n4");
  });

  it("walks visual rows when lines wrap (F6)", () => {
    // Line 0 "aaaa bbbb cccc" wraps into rows starting at 0, 5 and 10.
    const layout: RowLayout = { rowStarts: (line) => (line === 0 ? [0, 5, 10] : [0]) };
    expect(moved("aa|aa bbbb cccc\nxyz", "down", false, { layout })).toBe("aaaa bb|bb cccc\nxyz");
    expect(moved("aaaa bbbb cc|cc\nxyz", "up", false, { layout })).toBe("aaaa bb|bb cccc\nxyz");
    expect(moved("aaaa bbbb cc|cc\nxyz", "down", false, { layout })).toBe("aaaa bbbb cccc\nxy|z");
    expect(moved("aaaa bbbb cccc\nx|yz", "up", false, { layout })).toBe("aaaa bbbb c|ccc\nxyz");
  });

  it("⌘←/→ stop at the visual row first, then at the logical line (F6)", () => {
    const layout: RowLayout = { rowStarts: () => [0, 5, 10] };
    expect(moved("aaaa bb|bb cccc", "lineStart", false, { layout })).toBe("aaaa |bbbb cccc");
    expect(moved("aaaa |bbbb cccc", "lineStart", false, { layout })).toBe("|aaaa bbbb cccc");
    expect(moved("aaaa bb|bb cccc", "lineEnd", false, { layout })).toBe("aaaa bbbb| cccc");
    expect(moved("aaaa bbbb| cccc", "lineEnd", false, { layout })).toBe("aaaa bbbb cccc|");
  });
});

describe("typing and deleting", () => {
  it("replaces the selection with what is typed", () => {
    expect(after("a^bc|d", { type: "insert", text: "X" })).toBe("aX|d");
  });

  it("keeps the indentation on a new line, up to the cursor", () => {
    expect(after("    foo|", { type: "newline" })).toBe("    foo\n    |");
    expect(after("  |  foo", { type: "newline" })).toBe("  \n  |  foo");
  });

  it("⌫ removes a grapheme, a line break, or the selection", () => {
    expect(after("a😀|b", { type: "deleteBackward", unit: "char" })).toBe("a|b");
    expect(after("a\n|b", { type: "deleteBackward", unit: "char" })).toBe("a|b");
    expect(after("a^bc|d", { type: "deleteBackward", unit: "char" })).toBe("a|d");
  });

  it("⌫ in leading spaces goes back to the previous indentation stop", () => {
    expect(afterIndented("      |x", { type: "deleteBackward", unit: "char" })).toBe("    |x");
    expect(afterIndented("    |x", { type: "deleteBackward", unit: "char" })).toBe("|x");
  });

  it("⌥⌫ and ⌘⌫ remove a word and the line up to the cursor", () => {
    expect(after("let foo|", { type: "deleteBackward", unit: "word" })).toBe("let |");
    expect(after("let foo|", { type: "deleteBackward", unit: "line" })).toBe("|");
    expect(after("a\n|b", { type: "deleteBackward", unit: "line" })).toBe("a|b");
  });

  it("⌦ and ⌥⌦ remove forwards", () => {
    expect(after("a|bc", { type: "deleteForward", unit: "char" })).toBe("a|c");
    expect(after("|foo bar", { type: "deleteForward", unit: "word" })).toBe("| bar");
    expect(after("a|\nb", { type: "deleteForward", unit: "char" })).toBe("a|b");
  });
});

describe("selection commands", () => {
  it("⌘A selects everything", () => {
    expect(after("a\nb|c", { type: "selectAll" })).toBe("^a\nbc|");
  });

  it("⌘L selects the line, then grows by one", () => {
    const doc = docFrom("one\ntw|o\nthree");
    run(doc, { type: "selectLine" }, ctx());
    expect(show(doc)).toBe("one\n^two\n|three");
    run(doc, { type: "selectLine" }, ctx());
    expect(show(doc)).toBe("one\n^two\nthree|");
  });
});

describe("indentation", () => {
  it("⇥ at a cursor fills up to the next stop", () => {
    expect(after("ab|", { type: "indent" })).toBe("ab  |");
    expect(after("|x", { type: "indent" })).toBe("    |x");
  });

  it("⇥ and ⇧⇥ on a selection work line by line and skip empty lines", () => {
    expect(after("^a\n\nb|", { type: "indent" })).toBe("    ^a\n\n    b|");
    expect(afterIndented("    ^a\n  b\nc|", { type: "outdent" })).toBe("^a\nb\nc|");
  });

  it("uses tabs in a tab-indented file", () => {
    expect(after("\tone\n\t|two", { type: "indent" })).toBe("\tone\n\t\t|two");
  });

  it("leaves a last line out that the selection only reaches at column 0", () => {
    expect(after("^a\nb\n|c", { type: "indent" })).toBe("    ^a\n    b\n|c");
  });
});

describe("line commands", () => {
  it("⌥↑/↓ move the selected lines", () => {
    expect(after("one\ntw|o\nthree", { type: "moveLines", dir: -1 })).toBe("tw|o\none\nthree");
    expect(after("one\ntw|o\nthree", { type: "moveLines", dir: 1 })).toBe("one\nthree\ntw|o");
    expect(after("on|e\ntwo", { type: "moveLines", dir: -1 })).toBe("on|e\ntwo");
    expect(after("^one\ntwo|\nthree", { type: "moveLines", dir: 1 })).toBe("three\n^one\ntwo|");
  });

  it("⇧⌥↑/↓ duplicate them, the cursor going with the copy", () => {
    expect(after("a|b\nc", { type: "duplicateLines", dir: 1 })).toBe("ab\na|b\nc");
    expect(after("a|b\nc", { type: "duplicateLines", dir: -1 })).toBe("a|b\nab\nc");
  });

  it("⌘/ comments at the smallest indentation and back", () => {
    expect(after("^  a\n    b|", { type: "toggleComment" })).toBe("^  // a\n  //   b|");
    expect(after("^  // a\n  //   b|", { type: "toggleComment" })).toBe("^  a\n    b|");
    expect(after("  a|", { type: "toggleComment" }, { commentPrefix: null })).toBe("  a|");
  });

  it("picks the comment prefix by extension", () => {
    expect(commentPrefixFor("main.rs")).toBe("//");
    expect(commentPrefixFor("Cargo.TOML")).toBe("#");
    expect(commentPrefixFor("notes.md")).toBeNull();
  });
});

describe("clipboard", () => {
  it("copies the selection, or the whole line without one", () => {
    expect(copyText(docFrom("a^bc|d"))).toEqual({ text: "bc", wholeLine: false });
    expect(copyText(docFrom("one\ntw|o"))).toEqual({ text: "two\n", wholeLine: true });
  });

  it("cuts a whole line with its break and lands in the next one", () => {
    const doc = docFrom("one\ntw|o\nthree");
    expect(cut(doc, ctx())).toEqual({ text: "two\n", wholeLine: true });
    expect(show(doc)).toBe("one\nth|ree");
    const last = docFrom("one\ntw|o");
    cut(last, ctx());
    expect(show(last)).toBe("on|e");
  });

  it("pastes a whole line above the current one", () => {
    const doc = docFrom("one\nt|wo");
    paste(doc, { text: "new\n", wholeLine: true }, ctx());
    expect(show(doc)).toBe("one\nnew\nt|wo");
  });

  it("pastes other text in place of the selection, as one undo step", () => {
    const doc = docFrom("a^b|c");
    paste(doc, { text: "X\nY", wholeLine: false }, ctx());
    expect(show(doc)).toBe("aX\nY|c");
    doc.undo();
    expect(show(doc)).toBe("a^b|c");
  });
});
