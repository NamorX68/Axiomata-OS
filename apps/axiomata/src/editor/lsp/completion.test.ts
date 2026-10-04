import { describe, expect, it } from "vitest";

import { run, UNWRAPPED } from "../commands";
import { EditorDocument } from "../document";
import { cursor, pos, range } from "../position";
import {
  completionEdit,
  expandSnippet,
  filterItems,
  kindTag,
  matchScore,
  parseCompletion,
  parseItem,
  type CompletionItem,
} from "./completion";

const r = (l1: number, c1: number, l2: number, c2: number) => ({
  start: { line: l1, character: c1 },
  end: { line: l2, character: c2 },
});

function item(label: string, extra: Partial<CompletionItem> = {}): CompletionItem {
  return { ...parseItem({ label })!, ...extra };
}

describe("parseCompletion", () => {
  it("reads a list with defaults, a bare array, and nothing", () => {
    const list = parseCompletion({
      isIncomplete: true,
      itemDefaults: { editRange: r(0, 2, 0, 4), insertTextFormat: 2 },
      items: [
        { label: "push", kind: 2, labelDetails: { detail: "(value)" }, insertText: "push(${1:value})" },
        { label: "len", detail: "fn len(&self) -> usize", insertTextFormat: 1 },
        { nope: true },
      ],
    });
    expect(list.incomplete).toBe(true);
    expect(list.items).toHaveLength(2);
    expect(list.items[0]).toMatchObject({ detail: "(value)", kind: 2, snippet: true, insertText: "push(${1:value})" });
    expect(list.items[0].range).toEqual(range(pos(0, 2), pos(0, 4)));
    expect(list.items[1]).toMatchObject({ detail: "fn len(&self) -> usize", snippet: false });
    expect(parseCompletion([{ label: "a" }]).items.map((i) => i.label)).toEqual(["a"]);
    expect(parseCompletion(null)).toEqual({ items: [], incomplete: false });
  });

  it("takes a text edit (the insert range of an insert/replace edit) and extra edits", () => {
    const parsed = parseItem({
      label: "HashMap",
      textEdit: { insert: r(3, 4, 3, 7), replace: r(3, 4, 3, 12), newText: "HashMap" },
      additionalTextEdits: [{ range: r(0, 0, 0, 0), newText: "use std::collections::HashMap;\n" }, { bad: 1 }],
      documentation: { kind: "markdown", value: "A **map**." },
    })!;
    expect(parsed.range).toEqual(range(pos(3, 4), pos(3, 7)));
    expect(parsed.additionalEdits).toEqual([
      { range: range(pos(0, 0), pos(0, 0)), text: "use std::collections::HashMap;\n" },
    ]);
    expect(parsed.documentation).toBe("A **map**.");
    expect(parseItem({ label: "x", documentation: { kind: "plaintext", value: "a*b" } })!.documentation).toBe("a\\*b");
  });

  it("names kinds briefly", () => {
    expect(kindTag(3)).toBe("fn");
    expect(kindTag(null)).toBe("");
    expect(kindTag(99)).toBe("");
  });
});

describe("matching", () => {
  it("needs every character in order and prefers starts, word starts and runs", () => {
    expect(matchScore("psh", "push")).not.toBeNull();
    expect(matchScore("hsp", "push")).toBeNull();
    expect(matchScore("hm", "HashMap")!).toBeGreaterThan(matchScore("hm", "thumb")!);
    expect(matchScore("to_s", "to_string")!).toBeGreaterThan(matchScore("to_s", "top_ups")!);
  });

  it("ranks by score, then the server's sort text; nothing typed keeps the server's order", () => {
    const items = [
      item("into_iter", { sortText: "b" }),
      item("iter", { sortText: "c" }),
      item("is_empty", { sortText: "a" }),
    ];
    expect(filterItems(items, "iter").map((i) => i.label)).toEqual(["iter", "into_iter"]);
    expect(filterItems(items, "").map((i) => i.label)).toEqual(["is_empty", "into_iter", "iter"]);
  });
});

describe("snippets (L9)", () => {
  it("inserts placeholders as their text and puts the cursor on the first stop", () => {
    expect(expandSnippet("push(${1:value})$0")).toEqual({ text: "push(value)", cursor: 5 });
    expect(expandSnippet("fn ${1:name}(${2}) {\n\t$0\n}")).toEqual({ text: "fn name() {\n\t\n}", cursor: 3 });
    expect(expandSnippet("if $0 {}")).toEqual({ text: "if  {}", cursor: 3 });
    expect(expandSnippet("${1|a,b|} x")).toEqual({ text: "a x", cursor: 0 });
    expect(expandSnippet("${1:outer ${2:inner}}")).toEqual({ text: "outer inner", cursor: 0 });
    expect(expandSnippet("cost \\$5 ${TM_FILENAME:file}")).toEqual({ text: "cost $5 file", cursor: 12 });
    expect(expandSnippet("plain")).toEqual({ text: "plain", cursor: 5 });
  });
});

describe("completionEdit and the complete command", () => {
  const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 10, commentPrefix: null, now: 0 };

  function doc(text: string, at: { line: number; col: number }): EditorDocument {
    const d = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
    d.setSelection(cursor(pos(at.line, at.col)));
    return d;
  }

  it("replaces the typed word, adds an import above, and lands where the snippet says — one undo step", () => {
    const d = doc("fn main() {\n    let m = Has\n}", { line: 1, col: 15 });
    const it = item("HashMap", {
      insertText: "HashMap::new($1)",
      snippet: true,
      additionalEdits: [{ range: range(pos(0, 0), pos(0, 0)), text: "use std::collections::HashMap;\n" }],
    });
    const edit = completionEdit(it, pos(1, 15), 12, "    ");
    expect(edit).toMatchObject({ before: 3, after: 0, text: "HashMap::new()", cursor: 13 });
    run(d, { type: "complete", edit }, ctx);
    expect(d.store.text()).toBe("use std::collections::HashMap;\nfn main() {\n    let m = HashMap::new()\n}");
    expect(d.selection.head).toEqual(pos(2, 25));
    d.undo();
    expect(d.store.text()).toBe("fn main() {\n    let m = Has\n}");
  });

  it("puts the cursor right after several extra edits, one on its own line", () => {
    const d = doc("a\nb c", { line: 1, col: 3 });
    const edit = {
      before: 1,
      after: 0,
      text: "cat",
      cursor: 3,
      extra: [
        { range: range(pos(1, 0), pos(1, 1)), text: "x\ny" },
        { range: range(pos(0, 0), pos(0, 0)), text: "use z;\n" },
      ],
    };
    run(d, { type: "complete", edit }, ctx);
    expect(d.store.text()).toBe("use z;\na\nx\ny cat");
    expect(d.selection.head).toEqual(pos(3, 5));
  });

  it("uses the server's range on the cursor's line, indents further lines, and drops overlapping extras", () => {
    const it = item("match", {
      insertText: "match ${1:x} {\n\t$0\n}",
      snippet: true,
      range: range(pos(2, 4), pos(2, 8)),
      additionalEdits: [{ range: range(pos(2, 5), pos(2, 6)), text: "!" }],
    });
    const edit = completionEdit(it, pos(2, 7), 7, "    ");
    expect(edit).toEqual({ before: 3, after: 1, text: "match x {\n    \t\n    }", cursor: 6, extra: [] });
  });
});
