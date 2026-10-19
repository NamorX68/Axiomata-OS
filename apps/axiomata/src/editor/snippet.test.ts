import { describe, expect, it } from "vitest";

import { completionTargets, run, UNWRAPPED } from "./commands";
import { EditorDocument } from "./document";
import { completionEdit, type CompletionItem } from "./lsp/completion";
import { cursor, pos, range } from "./position";
import { SnippetSession } from "./snippet";

const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 10, commentPrefix: null, now: 0 };

function snippetItem(insertText: string): CompletionItem {
  return {
    label: "s",
    detail: "",
    kind: 15,
    filterText: "s",
    sortText: "s",
    insertText,
    snippet: true,
    range: null,
    additionalEdits: [],
    documentation: null,
    preselect: false,
    raw: null,
  };
}

/** A document with `fo` typed, the snippet taken there, and a session over its stops. */
function taken(insertText: string): { d: EditorDocument; session: SnippetSession } {
  const d = new EditorDocument("x fo", { indentFallback: { kind: "spaces", size: 4 } });
  d.setSelection(cursor(pos(0, 4)));
  const edit = completionEdit(snippetItem(insertText), pos(0, 4), 2, "");
  const { stops } = completionTargets(d, edit);
  run(d, { type: "complete", edit }, ctx);
  return { d, session: new SnippetSession(d, stops) };
}


describe("snippet tab stops (#36)", () => {
  it("selects the first placeholder, so typing replaces it", () => {
    const { d } = taken("for ${1:item} in ${2:list} {$0}");
    expect(d.store.text()).toBe("x for item in list {}");
    expect(d.selection).toEqual({ anchor: pos(0, 6), head: pos(0, 10) });
    run(d, { type: "insert", text: "row", kind: "typing" }, ctx);
    expect(d.store.text()).toBe("x for row in list {}");
  });

  it("follows the text: a replaced and grown placeholder stays one stop, the next one moves along", () => {
    const { d, session } = taken("for ${1:item} in ${2:list} {$0}");
    run(d, { type: "insert", text: "r", kind: "typing" }, ctx);
    run(d, { type: "insert", text: "ows", kind: "typing" }, ctx);
    expect(d.store.text()).toBe("x for rows in list {}");
    expect(session.holds(d.selection)).toBe(true);
    expect(session.current).toEqual(range(pos(0, 6), pos(0, 10)));
    expect(session.next()).toEqual(range(pos(0, 14), pos(0, 18)));
    expect(session.atLast).toBe(false);
    expect(session.next()).toEqual(range(pos(0, 20), pos(0, 20)));
    expect(session.atLast).toBe(true);
    expect(session.next()).toBeNull();
    expect(session.previous()).toEqual(range(pos(0, 14), pos(0, 18)));
    session.dispose();
  });

  it("grows an empty stop with what is typed into it", () => {
    const { d, session } = taken("f(${1})$0");
    expect(d.selection).toEqual({ anchor: pos(0, 4), head: pos(0, 4) });
    run(d, { type: "insert", text: "ab", kind: "typing" }, ctx);
    expect(session.current).toEqual(range(pos(0, 4), pos(0, 6)));
    expect(session.next()).toEqual(range(pos(0, 7), pos(0, 7)));
  });

  it("keeps the end stop after what is typed into the last placeholder", () => {
    const { d, session } = taken("for ${1:a} in ${2:b}");
    expect(session.next()).toEqual(range(pos(0, 11), pos(0, 12)));
    d.setSelection({ anchor: pos(0, 11), head: pos(0, 12) });
    run(d, { type: "insert", text: "x", kind: "typing" }, ctx);
    run(d, { type: "insert", text: "y", kind: "typing" }, ctx);
    expect(d.store.text()).toBe("x for a in xy");
    expect(session.next()).toEqual(range(pos(0, 13), pos(0, 13)));
    expect(session.previous()).toEqual(range(pos(0, 11), pos(0, 13)));
    expect(session.previous()).toEqual(range(pos(0, 6), pos(0, 7)));
    expect(session.previous()).toBeNull();
  });

  it("no longer holds a cursor outside every stop", () => {
    const { d, session } = taken("for ${1:item} in ${2:list} {$0}");
    d.setSelection(cursor(pos(0, 0)));
    expect(session.holds(d.selection)).toBe(false);
  });

  it("a plain completion has no stops and lands at its end", () => {
    const d = new EditorDocument("fo", { indentFallback: { kind: "spaces", size: 4 } });
    d.setSelection(cursor(pos(0, 2)));
    const edit = completionEdit({ ...snippetItem("foobar"), snippet: false }, pos(0, 2), 0, "");
    const targets = completionTargets(d, edit);
    expect(targets.stops).toEqual([]);
    expect(targets.selection).toEqual(cursor(pos(0, 6)));
  });
});
