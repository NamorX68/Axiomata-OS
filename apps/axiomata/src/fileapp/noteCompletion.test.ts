import { describe, expect, it, vi } from "vitest";

import { run, UNWRAPPED } from "../editor/commands";
import { EditorDocument } from "../editor/document";
import { cursor, pos, type Pos } from "../editor/position";
import { CompletionMenu, type CompletionPort } from "./completionMenu";
import { withFileWords } from "./fileWords";
import { noteCompletion, noteItems, noteSpanAt, tagItems } from "./noteCompletion";
import { NoteIndex } from "./noteLinks";

const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 10, commentPrefix: null };

function doc(text: string, at: Pos): EditorDocument {
  const d = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
  d.setSelection(cursor(at));
  return d;
}

function type(d: EditorDocument, text: string, menu: CompletionMenu): void {
  for (const ch of text) {
    run(d, { type: "insert", text: ch }, ctx);
    menu.typed(d, ch);
  }
}

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve();
}

const index = new NoteIndex([
  { path: "KI/Modelle.md", area: "KI", is_markdown: true, tags: ["ki-modelle"] },
  { path: "Arbeit/Plan.md", area: "Arbeit", is_markdown: true, tags: ["arbeit"] },
  { path: "Persönlich/Plan.md", area: "Persönlich", is_markdown: true },
  { path: "Inbox/Heute.md", area: "Inbox", is_markdown: true },
  { path: "Learning/page.html", area: "Learning", is_markdown: false },
]);
const sources = {
  notes: async () => index,
  tagList: async () => [
    { tag: "rust", description: "The Rust language" },
    { tag: "ki-modelle", description: "AI models" },
  ],
};

/** The menu as the surface builds it: the note source over a (here absent) server, with the file's words. */
function setup(text: string, at: Pos, server: CompletionPort | null = null) {
  const d = doc(text, at);
  const port = noteCompletion(server, d, "Inbox/Heute.md", sources);
  const words = vi.fn(() => ["wordfromfile"]);
  const menu = new CompletionMenu(withFileWords(port, words), () => {});
  return { d, menu, words };
}

describe("noteSpanAt", () => {
  it("finds a note name after [[ and a tag after #", () => {
    expect(noteSpanAt("see [[Mod", 9)).toMatchObject({ kind: "link", start: 6 });
    expect(noteSpanAt("see [[", 6)).toMatchObject({ kind: "link", start: 6 });
    expect(noteSpanAt("tag #ru", 7)).toMatchObject({ kind: "tag", start: 5 });
    expect(noteSpanAt("#", 1)).toMatchObject({ kind: "tag", start: 1 });
  });

  it("finds none past the link's end, mid-word, or in inline code", () => {
    expect(noteSpanAt("[[A]] x", 7)).toBeNull();
    expect(noteSpanAt("[[A|al", 6)).toBeNull();
    expect(noteSpanAt("C#", 2)).toBeNull();
    expect(noteSpanAt("`[[x", 4)).toBeNull();
    expect(noteSpanAt("## ", 3)).toBeNull();
  });
});

describe("noteItems / tagItems", () => {
  it("offers the vault's Markdown notes, not the note itself, by the shortest link that finds them", () => {
    const shown = noteItems(index, "Inbox/Heute.md").map((i) => [i.label, i.detail, i.insertText]);
    expect(shown).toEqual([
      ["Modelle", "KI", "Modelle]]"],
      ["Plan", "Arbeit", "Arbeit/Plan]]"],
      ["Plan", "Persönlich", "Persönlich/Plan]]"],
    ]);
  });

  it("puts the tag list first, then the vault's other tags", () => {
    const shown = tagItems([{ tag: "#Rust", description: "Lang" }], ["arbeit", "rust"]).map((i) => [i.label, i.detail]);
    expect(shown).toEqual([
      ["rust", "Lang"],
      ["arbeit", "in the vault"],
    ]);
  });
});

describe("note completion through the menu (Q4)", () => {
  it("opens after [[, follows a name with spaces, and writes the link", async () => {
    const { d, menu, words } = setup("", pos(0, 0));
    type(d, "[[", menu);
    await settle();
    expect(menu.view?.anchor).toEqual(pos(0, 2));
    expect(menu.view?.items.map((i) => i.label)).toEqual(["Modelle", "Plan", "Plan"]);
    type(d, "pl ar", menu);
    await settle();
    expect(menu.view?.items.map((i) => i.detail)).toEqual(["Arbeit"]);
    const edit = await menu.take(d);
    expect(edit).toMatchObject({ before: 5, text: "Arbeit/Plan]]" });
    // The file's words never mix into a note name.
    expect(words).not.toHaveBeenCalled();
  });

  it("does not close the link twice when ]] is already there", async () => {
    const { d, menu } = setup("[[]]", pos(0, 2));
    type(d, "m", menu);
    await settle();
    expect(await menu.take(d)).toMatchObject({ text: "Modelle", before: 1, after: 0 });
  });

  it("closes once the link is closed", async () => {
    const { d, menu } = setup("", pos(0, 0));
    type(d, "[[Mo", menu);
    await settle();
    expect(menu.isOpen).toBe(true);
    type(d, "]", menu);
    expect(menu.isOpen).toBe(false);
  });

  it("offers tags after #, with / and - in them", async () => {
    const { d, menu } = setup("", pos(0, 0));
    type(d, "#ki-m", menu);
    await settle();
    expect(menu.view?.items.map((i) => i.label)).toEqual(["ki-modelle"]);
    expect(await menu.take(d)).toMatchObject({ before: 4, text: "ki-modelle" });
  });

  it("stays out of code blocks, where the file's words answer as before", async () => {
    const { d, menu, words } = setup("```\n\n```", pos(1, 0));
    type(d, "#wo", menu);
    await settle();
    expect(menu.view?.items.map((i) => i.label)).toEqual(["wordfromfile"]);
    expect(words).toHaveBeenCalled();
  });

  it("leaves everything else to the server", async () => {
    const ask = vi.fn(async () => ({ items: [], incomplete: false }));
    const server: CompletionPort = { ask, resolve: async (i) => i, triggers: () => [] };
    const { d, menu } = setup("", pos(0, 0), server);
    type(d, "wo", menu);
    await settle();
    expect(ask).toHaveBeenCalled();
    type(d, " [[", menu);
    await settle();
    expect(ask.mock.calls.every((call) => (call as unknown[])[3] === undefined)).toBe(true);
  });
});
