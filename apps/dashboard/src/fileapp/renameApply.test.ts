import { get } from "svelte/store";
import { describe, expect, it } from "vitest";

import { run, UNWRAPPED } from "../editor/commands";
import { EditorDocument } from "../editor/document";
import { parseWorkspaceEdit } from "../editor/lsp/client";
import { pos, range } from "../editor/position";
import type { TextEdit } from "../editor/textEdits";
import { applyRename, docTouched, documentChanges } from "./renameApply";

const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 1, commentPrefix: null };
const doc = (text: string) => new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
const at = (line: number, from: number, to: number, text: string): TextEdit => ({
  range: range(pos(line, from), pos(line, to)),
  text,
});

describe("parseWorkspaceEdit", () => {
  const r = { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } };

  it("reads `changes` and `documentChanges`, collecting edits per file", () => {
    const fromChanges = parseWorkspaceEdit({ changes: { "file:///a": [{ range: r, newText: "bar" }] } });
    expect(fromChanges.get("file:///a")).toEqual([{ range: range(pos(0, 0), pos(0, 3)), text: "bar" }]);
    const fromDocs = parseWorkspaceEdit({
      documentChanges: [
        { textDocument: { uri: "file:///a", version: 1 }, edits: [{ range: r, newText: "x" }] },
        { textDocument: { uri: "file:///a", version: 1 }, edits: [{ range: r, newText: "y" }] },
      ],
    });
    expect(fromDocs.get("file:///a")?.map((e) => e.text)).toEqual(["x", "y"]);
    expect(parseWorkspaceEdit(null).size).toBe(0);
  });

  it("refuses an edit that would create, move or delete files (L16)", () => {
    expect(() => parseWorkspaceEdit({ documentChanges: [{ kind: "rename", oldUri: "a", newUri: "b" }] })).toThrow(
      /files/,
    );
  });
});

describe("applyRename (ED6.5, L16)", () => {
  it("changes this editor, another open editor, and a closed file; skips what it may not touch", async () => {
    const here = doc("fn old() {}\nold();");
    const other = doc("use crate::old;");
    const applied: unknown[] = [];
    const disk: Record<string, string> = { "c.rs": "old()\r\nold()\r\n", "d.rs": "old" };
    const touchedBefore = get(docTouched);
    const edits = new Map<string, TextEdit[]>([
      ["file:///p/a.rs", [at(0, 3, 6, "new"), at(1, 0, 3, "new")]],
      ["file:///p/b.rs", [at(0, 11, 14, "new")]],
      ["file:///p/c.rs", [at(0, 0, 3, "new"), at(1, 0, 3, "new")]],
      ["file:///p/d.rs", [at(0, 0, 3, "new")]],
      ["file:///opt/lib.rs", [at(0, 0, 3, "new")]],
    ]);
    const outcome = await applyRename(edits, {
      here: {
        uri: "file:///p/a.rs",
        doc: here,
        apply: (changes) => {
          applied.push(changes);
          run(here, { type: "replaceText", changes }, ctx);
        },
      },
      openDoc: (uri) => (uri === "file:///p/b.rs" ? other : null),
      fileOf: (uri) =>
        uri.startsWith("file:///p/") ? { root: "project:1", rel: uri.slice("file:///p/".length) } : null,
      read: async (_root, rel) => ({ content: disk[rel], version: `v-${rel}` }),
      write: async (_root, rel, content, expected) => {
        if (rel === "d.rs") throw { kind: "Conflict", message: "changed" };
        expect(expected).toBe(`v-${rel}`);
        disk[rel] = content;
      },
    });
    expect(here.store.text()).toBe("fn new() {}\nnew();");
    expect(applied).toHaveLength(1);
    expect(other.store.text()).toBe("use crate::new;");
    expect(disk["c.rs"]).toBe("new()\r\nnew()\r\n");
    expect(outcome.changed).toBe(3);
    expect(outcome.skipped).toEqual([
      { file: "d.rs", reason: "changed on disk meanwhile" },
      { file: "file:///opt/lib.rs", reason: "outside the project" },
    ]);
    expect(get(docTouched)).toBe(touchedBefore + 1);
    // One undo step in each open document.
    other.undo();
    expect(other.store.text()).toBe("use crate::old;");
  });

  it("works on the text as the server has it: CRLF and the final line break do not shift anything", () => {
    const d = doc("a old\r\nold\r\n");
    const changes = documentChanges(d, [at(0, 2, 5, "new"), at(1, 0, 3, "new")]);
    run(d, { type: "replaceText", changes }, ctx);
    expect(d.textForSave()).toBe("a new\r\nnew\r\n");
  });
});
