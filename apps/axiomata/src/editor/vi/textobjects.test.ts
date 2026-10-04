/**
 * Direct tests of `textObject()` (`docs/plans/editor.md`, ED3, V9), as a
 * complement to the machine-level `"operators with motions and objects"`
 * table in `vi.test.ts`: sentences (`is`/`as`), paragraphs with a count,
 * quote whitespace rules, brackets across lines, and nested tags.
 */

import { describe, expect, it } from "vitest";

import { docFrom } from "../testing";
import { pos, range, type Pos } from "../position";
import { textObject } from "./textobjects";

/** The text a range covers, read back from the document for a plain assertion. */
function text(doc: ReturnType<typeof docFrom>, r: { start: Pos; end: Pos }) {
  return doc.store.slice(range(r.start, r.end));
}

describe("is/as — sentences", () => {
  it("takes the sentence up to the next `. ! ?`, inner without trailing space", () => {
    const doc = docFrom("One. Two three. Four.");
    const obj = textObject(doc.store, pos(0, 6), "s", true, 1);
    expect(obj && text(doc, obj)).toBe("Two three.");
  });

  it("as includes the blank after the sentence", () => {
    const doc = docFrom("One. Two three. Four.");
    const obj = textObject(doc.store, pos(0, 6), "s", false, 1);
    expect(obj && text(doc, obj)).toBe("Two three. ");
  });

  it("is null on an empty line", () => {
    const doc = docFrom("");
    expect(textObject(doc.store, pos(0, 0), "s", true, 1)).toBeNull();
  });
});

describe("ip — paragraphs with a count", () => {
  it("one count takes just this paragraph's lines", () => {
    const doc = docFrom("a\nb\n\nc\nd\n\ne\nf");
    const obj = textObject(doc.store, pos(3, 0), "p", true, 1);
    expect(obj && text(doc, obj)).toBe("c\nd");
  });

  it("a higher count reaches one further block — the blank run after it, not the next paragraph", () => {
    const doc = docFrom("a\nb\n\nc\nd\n\ne\nf");
    const obj = textObject(doc.store, pos(0, 0), "p", true, 2);
    expect(obj && text(doc, obj)).toBe("a\nb\n");
  });

  it("ap with a count counts paragraph and blank-run blocks together (2ap: two of each)", () => {
    const doc = docFrom("a\nb\n\nc\nd\n\ne\nf");
    const obj = textObject(doc.store, pos(0, 0), "p", false, 2);
    expect(obj && text(doc, obj)).toBe("a\nb\n\nc\nd\n");
  });
});

describe("a' — quote whitespace rules", () => {
  it("takes the blanks after the closing quote when there are some", () => {
    const doc = docFrom("say 'hi'  now");
    const obj = textObject(doc.store, pos(0, 6), "'", false, 1);
    expect(obj && text(doc, obj)).toBe("'hi'  ");
  });

  it("falls back to the blanks before the opening quote when none follow", () => {
    const doc = docFrom("say  'hi'now");
    const obj = textObject(doc.store, pos(0, 7), "'", false, 1);
    expect(obj && text(doc, obj)).toBe("  'hi'");
  });

  it("is null with no quote pair on the line", () => {
    const doc = docFrom("no quotes here");
    expect(textObject(doc.store, pos(0, 0), "'", true, 1)).toBeNull();
  });
});

describe("brackets across lines", () => {
  it("i{ takes the lines between whole, when the braces stand alone on their own lines", () => {
    const doc = docFrom("fn f() {\n    a();\n    b();\n}");
    const obj = textObject(doc.store, pos(1, 4), "{", true, 1);
    expect(obj?.linewise).toBe(true);
    expect(obj && text(doc, obj)).toBe("    a();\n    b();");
  });

  it("i{ on an empty body between the braces yields an empty, non-linewise range", () => {
    const doc = docFrom("fn f() {\n}");
    const obj = textObject(doc.store, pos(0, 7), "{", true, 1);
    expect(obj?.linewise).toBe(false);
    expect(obj && text(doc, obj)).toBe("");
  });

  it("i( across lines that are not brace-alone takes exactly the inside, spanning the line break", () => {
    const doc = docFrom("call(a,\n     b)");
    const obj = textObject(doc.store, pos(0, 6), "(", true, 1);
    expect(obj && text(doc, obj)).toBe("a,\n     b");
  });

  it("a count reaches an outer bracket pair", () => {
    const doc = docFrom("(a (b) c)");
    const obj = textObject(doc.store, pos(0, 4), "(", true, 2);
    expect(obj && text(doc, obj)).toBe("a (b) c");
  });

  it("is null when the cursor is outside any bracket pair", () => {
    const doc = docFrom("no brackets here");
    expect(textObject(doc.store, pos(0, 0), "(", true, 1)).toBeNull();
  });

  it("is null when the closing bracket comes before the cursor (an unmatched opener)", () => {
    const doc = docFrom("(a) b");
    expect(textObject(doc.store, pos(0, 4), "(", true, 1)).toBeNull();
  });
});

describe("it/at — nested tags", () => {
  it("takes the innermost element around the cursor", () => {
    const doc = docFrom("<a><b>x</b></a>");
    const obj = textObject(doc.store, pos(0, 7), "t", true, 1);
    expect(obj && text(doc, obj)).toBe("x");
  });

  it("a count reaches an outer, same-nesting element", () => {
    const doc = docFrom("<a><b>x</b></a>");
    const obj = textObject(doc.store, pos(0, 7), "t", true, 2);
    expect(obj && text(doc, obj)).toBe("<b>x</b>");
  });

  it("at includes the tags themselves", () => {
    const doc = docFrom("<a><b>x</b></a>");
    const obj = textObject(doc.store, pos(0, 7), "t", false, 1);
    expect(obj && text(doc, obj)).toBe("<b>x</b>");
  });

  it("is null past the deepest nesting the cursor sits in", () => {
    const doc = docFrom("<a><b>x</b></a>");
    expect(textObject(doc.store, pos(0, 7), "t", true, 3)).toBeNull();
  });

  it("is null with no enclosing tag", () => {
    const doc = docFrom("plain text");
    expect(textObject(doc.store, pos(0, 0), "t", true, 1)).toBeNull();
  });
});
